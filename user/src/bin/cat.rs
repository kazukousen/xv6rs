#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

use xv6rs_user::{
    entry_point, println,
    syscall::{sys_close, sys_open, sys_read, sys_write},
    Args,
};

entry_point!(main);
fn main(args: &mut Args) -> Result<(), &'static str> {
    for arg in args.skip(1) {
        let fd = sys_open(arg, 0);
        cat(fd)?;
        sys_close(fd);
    }
    Ok(())
}

fn cat(fd: i32) -> Result<(), &'static str> {
    let mut buf = [0u8; 512];
    let mut n = sys_read(fd, &mut buf);
    while n > 0 {
        if sys_write(1, &buf) != n {
            return Err("write error");
        }
        n = sys_read(fd, &mut buf);
    }
    if n < 0 {
        return Err("read error");
    }
    Ok(())
}
