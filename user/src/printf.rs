use core::fmt::{self, Write};

use crate::syscall::sys_write;

struct StdIO;

impl fmt::Write for StdIO {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        sys_write(1, s.as_bytes());
        Ok(())
    }
}

pub fn _print(args: fmt::Arguments<'_>) {
    StdIO.write_fmt(args).expect("printf: error");
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::printf::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
