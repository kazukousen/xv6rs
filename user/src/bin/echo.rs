#![no_std]
#![no_main]

use xv6rs_user::{syscall::sys_exit, Args, print, println};

#[no_mangle]
extern "C" fn _start(argc: i32, argv: &*const u8) {
    if argc < 2 {
        sys_exit(1);
    }
    let mut args = Args::new(argc, argv).skip(1);

    let c = args.next().unwrap();
    print!("{}", c);

    for arg in args {
        print!(" {}", arg);
    }
    println!();

    sys_exit(0);
}

