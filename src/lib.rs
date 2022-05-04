#![no_std]
#![cfg_attr(test, no_main)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::{
    panic::PanicInfo,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::param::{QEMU_EXIT_FAIL, QEMU_EXIT_SUCCESS, QEMU_TEST0};

mod console;
mod param;
pub mod printf;
mod register;
mod start;
mod uart;

pub static PANICKED: AtomicBool = AtomicBool::new(false);

pub fn bootstrap() {
    console::init();
    println!("Hello, xv6 in Rust!");
}

#[no_mangle]
fn abort() -> ! {
    panic!("abort");
}

#[cfg(test)]
#[panic_handler]
pub fn panic(info: &PanicInfo<'_>) -> ! {
    test_panic_handler(info)
}

pub fn test_panic_handler(info: &PanicInfo<'_>) -> ! {
    println!("failed: {}", info);
    PANICKED.store(true, Ordering::Relaxed);
    unsafe { ptr::write_volatile(QEMU_TEST0 as *mut u32, QEMU_EXIT_FAIL) };
    loop {}
}

pub fn test_runner(tests: &[&dyn Testable]) {
    println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }

    println!("\x1b[0;32mall tests finished!\x1b[0m");
    unsafe { ptr::write_volatile(QEMU_TEST0 as *mut u32, QEMU_EXIT_SUCCESS) };
}

pub trait Testable {
    fn run(&self) -> ();
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self) {
        print!("{}...\t", core::any::type_name::<T>());
        self();
        println!("\x1b[0;32m[ok]\x1b[0m");
    }
}

#[cfg(test)]
#[no_mangle]
unsafe fn main() -> ! {
    bootstrap();
    test_main();
    loop {}
}

#[cfg(test)]
mod tests {
    #[test_case]
    fn trivial_assertion() {
        assert_eq!(1, 1);
    }
}
