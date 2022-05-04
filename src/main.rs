#![no_std]
#![no_main]

mod start;
mod register;
mod param;

use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
fn main() -> ! {
    loop {}
}
