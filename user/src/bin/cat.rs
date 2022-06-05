#![no_std]
#![no_main]

use xv6rs_user::{
    println,
    syscall::{sys_close, sys_exit, sys_open, sys_read, sys_write},
    Args,
};

#[no_mangle]
pub unsafe extern "C" fn _start(argc: i32, argv: &*const u8) -> ! {
    if argc <= 1 {
        println!("argc 0-1");
        sys_exit(0);
    }
    for arg in Args::new(argc, argv).skip(1) {
        let fd = sys_open(arg, 0);
        cat(fd);
        sys_close(fd);
    }
    sys_exit(0);
}

fn cat(fd: i32) {
    let mut buf = [0u8; 512];
    let mut n = sys_read(fd, &mut buf);
    while n > 0 {
        if sys_write(1, &buf) != n {
            println!("cat: write error");
            sys_exit(1);
        }
        n = sys_read(fd, &mut buf);
    }
    if n < 0 {
        println!("cat: read error");
        sys_exit(1);
    }
}
