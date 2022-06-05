#![no_std]
#![feature(alloc_error_handler)]

pub mod printf;
pub mod syscall;

use core::panic::PanicInfo;

use crate::syscall::sys_exit;

#[panic_handler]
pub fn panic(info: &PanicInfo<'_>) -> ! {
    println!("panic: {}", info);
    sys_exit(1)
}

#[no_mangle]
fn abort() -> ! {
    panic!("abort");
}
