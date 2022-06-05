#![no_std]
#![no_main]

use xv6rs_user::{println, syscall::sys_exit};

#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    println!("Hello, world!");
    sys_exit(0);
}
