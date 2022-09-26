#![no_std]
#![no_main]

use core::str::from_utf8_unchecked;

use xv6rs_user::{
    net::{SAFamily, SockAddr},
    println,
    syscall::{sys_bind, sys_close, sys_exit, sys_read, sys_socket, sys_write},
    Args,
};

#[no_mangle]
extern "C" fn _start(argc: i32, argv: *const *const u8) {
    if argc <= 1 {
        println!("argc 0-1");
        sys_exit(0);
    }

    let mut args = Args::new(argc, argv).skip(1);
    let port = args.next().unwrap();
    let port: u16 = port.parse().unwrap();

    match serve(port) {
        Ok(_) => {
            println!("success");
            sys_exit(0);
        }
        Err(msg) => {
            println!("udp-server: {}", msg);
            sys_exit(1);
        }
    }
}

fn serve(port: u16) -> Result<(), &'static str> {
    let fd = sys_socket(1, 1, 1);
    println!("sockfd: {}", fd);
    sys_bind(
        fd,
        &SockAddr {
            family: SAFamily::INET,
            port: 2000,
            addr: 0,
        },
    );

    let mut buf = [0u8; 1024];
    let mut n = sys_read(fd, &mut buf);

    println!("n={} msg={}", n, unsafe { from_utf8_unchecked(&buf) });

    sys_close(fd);

    Ok(())
}
