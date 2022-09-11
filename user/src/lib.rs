#![no_std]
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
