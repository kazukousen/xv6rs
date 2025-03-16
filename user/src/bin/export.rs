#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

use xv6rs_user::{
    entry_point,
    println,
    syscall::sys_setenv,
    Args,
};

entry_point!(main);

fn main(args: &mut Args) -> Result<i32, &'static str> {
    // Skip the command name
    args.next();
    
    // Check if any arguments are provided
    let first_arg = args.next();
    if first_arg.is_none() {
        println!("Usage: export NAME=VALUE");
        return Ok(1);
    }
    
    // Process the first argument
    process_arg(first_arg.unwrap())?;
    
    // Process remaining arguments
    for arg in args {
        process_arg(arg)?;
    }
    
    Ok(0)
}

fn process_arg(arg: &str) -> Result<(), &'static str> {
    // Find the '=' character
    let mut parts = arg.splitn(2, '=');
    
    match (parts.next(), parts.next()) {
        (Some(name), Some(value)) => {
            // Set the environment variable
            if sys_setenv(name, value, true) < 0 {
                println!("Failed to set environment variable: {}", name);
                return Err("Failed to set environment variable");
            }
            Ok(())
        },
        _ => {
            println!("Invalid format. Usage: export NAME=VALUE");
            Err("Invalid format")
        }
    }
}
