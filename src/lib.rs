#![no_std]

use core::sync::atomic::AtomicBool;

mod start;
mod register;
mod param;
mod uart;
mod printf;
mod console;

pub static PANICKED: AtomicBool = AtomicBool::new(false);

pub fn bootstrap() {
    console::init();
    println!("Hello, xv6 in Rust!");
}

#[no_mangle]
fn abort() -> ! {
    panic!("abort");
}

