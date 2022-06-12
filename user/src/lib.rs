#![no_std]
#![feature(alloc_error_handler)]

pub mod fcntl;
pub mod fstat;
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

pub struct Args<'a> {
    argc: usize,
    argv: &'a [&'a str],
    count: usize,
}

impl<'a> Args<'a> {
    pub fn new(argc: i32, argv: &'a [&'a str]) -> Self {
        Self {
            argc: argc as usize,
            argv,
            count: 0,
        }
    }
}

impl<'a> Iterator for Args<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        if self.count >= self.argc {
            return None;
        }

        let argv = self.argv as *const [&'a str] as *const *const u8;
        let args = unsafe { from_raw_parts(argv, self.argc) };
        let arg = unsafe { args.get_unchecked(self.count) };

        self.count += 1;

        let s = unsafe { from_utf8_unchecked(from_raw_parts(*arg, strlen(*arg))) };
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
