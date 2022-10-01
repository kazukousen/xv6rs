#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

use xv6rs_user::{println, syscall::sys_exit};

#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    #[cfg(test)]
    crate::test_main();

    println!("Hello, world!");
    sys_exit(0);
}
