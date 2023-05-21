#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::{ptr, slice::from_raw_parts};

use xv6rs_user::{
    entry_point,
    fcntl::{O_CREATE, O_RDONLY, O_RDWR, O_WRONLY},
    println,
    syscall::{sys_close, sys_mmap, sys_open, sys_read, sys_write},
    Args,
};

entry_point!(main);
fn main(_: &mut Args) -> Result<i32, &'static str> {
    let f = "mmaptest.tmp\0";
    make_file(f);
    let fd = sys_open(f, O_RDWR);
    if fd < 0 {
        return Err("open");
    }

    cat(fd)?;
    println!();

    let size = PAGESIZE + PAGESIZE;
    let buf = sys_mmap(ptr::null(), size, 1 << 1 | 1 << 2, 1 << 2, fd, 0);
    println!("mmap created!");
    let buf = unsafe { from_raw_parts(buf as *const u8, size) };
    println!("buf[0] {}", buf[0]);
    println!("buf[1] {}", buf[1]);

    println!("verify content");
    vaild_content(buf)?;

    Ok(0)
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

// create a file to be mapped, containing 1.5 pages of 'A' and half a page of zeros.
fn make_file(f: &'static str) -> Result<(), &'static str> {
    let fd = sys_open(f, O_WRONLY | O_CREATE);
    let buf = [b'A'; 1024];
    // write 1.5 page
    for _ in 0..6 {
        if sys_write(fd, &buf) < 0 {
            return Err("make_file failed to write");
        }
    }

    if sys_close(fd) < 0 {
        return Err("make_file faled to close");
    }

    Ok(())
}

const PAGESIZE: usize = 4096;

fn vaild_content(data: &[u8]) -> Result<(), &'static str> {
    for i in 0..(PAGESIZE + PAGESIZE / 2) {
        if data[i] != b'A' {
            return Err("content invalid. must be A");
        }
    }
    for i in (PAGESIZE + PAGESIZE / 2)..(PAGESIZE * 2) {
        if data[i] != 0 {
            return Err("content invalid. must be empty");
        }
    }

    println!("Succeed.");

    Ok(())
}
