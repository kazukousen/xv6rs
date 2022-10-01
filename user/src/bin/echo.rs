#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

use xv6rs_user::{entry_point, print, println, Args};

entry_point!(main);

fn main(args: &mut Args) -> Result<i32, &'static str> {
    let c = args.skip(1).next().ok_or_else(|| "missing args")?;
    print!("{}", c);

    for arg in args {
        print!(" {}", arg);
    }
    println!();

    Ok(0)
}
