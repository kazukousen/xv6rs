use core::num::Wrapping;

use crate::{
    proc::either_copy_in,
    process::PROCESS_TABLE,
    spinlock::SpinLock,
    uart::{self, UART_TX},
};

pub fn init() {
    uart::init();
}

pub fn putc(c: u8) {
    uart::putc_sync(c);
}

/// user write()s to the console go here.
pub fn write(is_user: bool, src: *const u8, n: usize) {
    for i in 0..n {
        let mut c = 0u8;
        either_copy_in(is_user, unsafe { src.offset(i as isize) }, &mut c, 1);
        unsafe {
            UART_TX.putc(c);
        }
    }
}

const INPUT_BUF: usize = 128;

struct Console {
    buf: [u8; INPUT_BUF],
    r: Wrapping<usize>, // read index
    w: Wrapping<usize>, // write index
    e: Wrapping<usize>, // edit index
}

impl Console {
    const fn new() -> Self {
        Self {
            buf: [0; INPUT_BUF],
            r: Wrapping(0),
            w: Wrapping(0),
            e: Wrapping(0),
        }
    }
}

static CONSOLE: SpinLock<Console> = SpinLock::new(Console::new());

pub fn intr(c: u8) {
    let mut cons = CONSOLE.lock();
    match c {
        _ => {
            if c != 0 && (cons.e - cons.r).0 < INPUT_BUF {
                let c = if c == CTRL_CR { CTRL_LF } else { c };
                // echo back to the user
                putc(c);
                cons.e += Wrapping(1);
                let i = cons.e.0 % INPUT_BUF;
                cons.buf[i] = c;
                if c == b'\n' || cons.e == cons.r + Wrapping(INPUT_BUF) {
                    cons.w = cons.e;
                    unsafe { PROCESS_TABLE.wakeup(&cons.r as *const Wrapping<usize> as usize) };
                }
            }
        }
    }
    drop(cons);
}

const CTRL_BS: u8 = 0x08;
const CTRL_LF: u8 = 0x0A;
const CTRL_CR: u8 = 0x0D;

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_write() {
        let src = [b'H', b'e', b'l', b'l', b'o', b'!', 0];
        write(false, src.as_ptr(), src.len());
    }
}
