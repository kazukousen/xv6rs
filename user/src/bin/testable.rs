#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::{mem, ptr, slice, str::from_utf8_unchecked};
use xv6rs_user::{
    entry_point,
    fcntl::O_RDWR,
    fstat::{DirEnt, FileStat, InodeType, DIRSIZ},
    println,
    syscall::{sys_dup, sys_exec, sys_fork, sys_fstat, sys_mknod, sys_open, sys_read, sys_wait},
    Args,
};

fn exec(buf: &[u8]) -> Result<(), &'static str> {
    let pid = sys_fork();
    if pid == 0 {
        sys_exec(unsafe { from_utf8_unchecked(&buf) });
        return Err("unreachable");
    }
    let mut status = 0i32;
    let wpid = sys_wait(&mut status);
    assert_eq!(pid, wpid);
    if status != 0 {
        return Err("test failed");
    }
    Ok(())
}

entry_point!(main);
fn main(args: &mut Args) -> Result<i32, &'static str> {
    if sys_open("test_console\0", O_RDWR) < 0 {
        sys_mknod("test_console\0", 1, 1);
        sys_open("test_console\0", O_RDWR);
    }

    sys_dup(0); // stdio
    sys_dup(0); // stderr

    let fd = sys_open(".\0", 0);
    if fd < 0 {
        return Err("open failed");
    }

    let mut st = FileStat::uninit();
    if sys_fstat(fd, &mut st) < 0 {
        return Err("stat failed");
    }

    match st.typ {
        InodeType::Directory => {}
        _ => {
            return Err("stat type invalid");
        }
    }

    let mut de = DirEnt::empty();
    let mut de_slice: &mut [u8] = unsafe {
        slice::from_raw_parts_mut(&mut de as *mut DirEnt as *mut u8, mem::size_of::<DirEnt>())
    };

    let mut buf = [0u8; 512];

    while sys_read(fd, &mut de_slice) == mem::size_of::<DirEnt>().try_into().unwrap() {
        if de.inum == 0 {
            continue;
        }

        let mut is_test = false;
        for i in (0..DIRSIZ - 4).rev() {
            is_test = de.name[i] == b'-'
                && de.name[i + 1] == b't'
                && de.name[i + 2] == b'e'
                && de.name[i + 3] == b's'
                && de.name[i + 4] == b't';
            if is_test {
                break;
            }
        }
        if !is_test {
            continue;
        }

        unsafe { ptr::copy_nonoverlapping(de.name.as_ptr(), buf.as_mut_ptr(), DIRSIZ) };

        println!("Testing {}", unsafe { from_utf8_unchecked(&de.name) });
        exec(&buf)?;
        println!();
    }

    Ok(0)
}
