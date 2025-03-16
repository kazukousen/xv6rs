#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

use xv6rs_user::{
    entry_point,
    println,
    syscall::sys_unsetenv,
    Args,
};

entry_point!(main);

fn main(args: &mut Args) -> Result<i32, &'static str> {
    // Skip the command name
    args.next();
    
    // Check if any arguments are provided
    let first_arg = args.next();
    if first_arg.is_none() {
        println!("Usage: unset NAME [NAME...]");
        return Ok(1);
    }
    
    // Process the first argument
    process_arg(first_arg.unwrap())?;
    
    // Process remaining arguments
    for name in args {
        process_arg(name)?;
    }
    
    Ok(0)
}

fn process_arg(name: &str) -> Result<(), &'static str> {
    // Unset the environment variable
    if sys_unsetenv(name) < 0 {
        println!("Failed to unset environment variable: {}", name);
        return Err("Failed to unset environment variable");
    }
    Ok(())
}
