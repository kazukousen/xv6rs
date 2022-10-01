#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

use xv6rs_user::{print, println, syscall::sys_exit, Args};

#[no_mangle]
extern "C" fn _start(argc: i32, argv: *const *const u8) {
    #[cfg(test)]
    crate::test_main();

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
