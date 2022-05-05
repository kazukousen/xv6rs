#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::{panic::PanicInfo, sync::atomic::Ordering};

use xv6rs::{println, PANICKED};

#[no_mangle]
fn main() -> ! {
    #[cfg(test)]
    test_main();

    xv6rs::bootstrap();

    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    println!("panic: {}", info);
    PANICKED.store(true, Ordering::Relaxed);
    loop {}
}
