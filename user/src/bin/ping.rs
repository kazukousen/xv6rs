#![no_std]
#![no_main]

use xv6rs_user::{
    net::{SAFamily, SockAddr},
    println,
    syscall::{sys_close, sys_connect, sys_exit, sys_socket, sys_write},
    Args,
};

#[no_mangle]
extern "C" fn _start(argc: i32, argv: *const *const u8) {
    if argc <= 1 {
        println!("argc 0-1");
        sys_exit(0);
    }

    let mut args = Args::new(argc, argv).skip(1);
    let dport = args.next().unwrap();
    let dport: u16 = dport.parse().unwrap();
    let msg = args.next().unwrap();

    match ping(dport, 3, msg) {
        Ok(_) => {
            println!("success");
            sys_exit(0);
        }
        Err(msg) => {
            println!("ping: {}", msg);
            sys_exit(1);
        }
    }
}

fn ping(dst_port: u16, attempts: usize, msg: &str) -> Result<(), &'static str> {
    let dst_ip = (10 << 24) | (0 << 16) | (2 << 8) | (2 << 0);

    let fd = sys_socket(1, 1, 1);
    sys_connect(
        fd,
        &SockAddr {
            family: SAFamily::INET,
            addr: dst_ip,
            port: 25601,
        },
    );

    for i in 0..attempts {
        if sys_write(fd, msg.as_bytes()) < 0 {
            return Err("write() failed");
        }
    }

    sys_close(fd);

    Ok(())
}
