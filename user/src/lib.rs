#![no_std]
#![cfg_attr(test, no_main)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]
#![feature(alloc_error_handler)]

pub mod fcntl;
pub mod fstat;
pub mod net;
pub mod printf;
pub mod syscall;

use core::{panic::PanicInfo, slice::from_raw_parts, str::from_utf8_unchecked};

use crate::syscall::sys_exit;

#[panic_handler]
pub fn panic(info: &PanicInfo<'_>) -> ! {
    #[cfg(test)]
    test_panic_handler(info);

    println!("panic: {}", info);
    sys_exit(1)
}

#[no_mangle]
fn abort() -> ! {
    panic!("abort");
}

pub struct Args {
    length: usize,
    args: &'static [*const u8],
    pos: usize,
}

impl Args {
    pub fn new(argc: i32, argv: *const *const u8) -> Self {
        Self {
            length: argc as usize,
            args: unsafe { from_raw_parts(argv, argc as usize) },
            pos: 0,
        }
    }
}

impl Iterator for Args {
    type Item = &'static str;
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.length {
            return None;
        }

        let arg: &*const u8 = unsafe { self.args.get_unchecked(self.pos) };

        self.pos += 1;

        let s: &[u8] = unsafe { from_raw_parts(*arg, strlen(*arg)) };
        let s: &str = unsafe { from_utf8_unchecked(s) };
        Some(s)
    }
}

pub fn strlen(mut c: *const u8) -> usize {
    let mut pos = 0;
    unsafe {
        while *c != 0 {
            pos += 1;
            c = c.offset(1);
        }
    }
    pos
}

#[macro_export]
macro_rules! entry_point {
    ($path:path) => {
        #[export_name = "_start"]
        pub extern "C" fn __impl_start(argc: i32, argv: *const *const u8) {
            #[cfg(test)]
            crate::test_main();

            let mut args = $crate::Args::new(argc, argv);

            let f: fn(&mut $crate::Args) -> Result<(), &'static str> = $path;

            match f(&mut args) {
                Ok(_) => {
                    $crate::syscall::sys_exit(0);
                }
                Err(msg) => {
                    $crate::println!("fatal: {}", msg);
                    $crate::syscall::sys_exit(1);
                }
            }
        }
    };
}

#[cfg(test)]
fn test_panic_handler(info: &PanicInfo<'_>) {
    println!("failed: {}", info);
    sys_exit(1);
}

pub fn test_runner(tests: &[&dyn Testable]) {
    println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }

    println!("\x1b[0;32mall tests finished!\x1b[0m");
    sys_exit(0);
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
extern "C" fn _start() {
    test_main();
}
