# xv6rs

xv6rs is a Rust implementation of the xv6 operating system, originally developed at MIT. While the original xv6 is written in C, this project reimplements it in Rust, targeting the RISC-V architecture.

## Overview

This project is a Rust port of the xv6 operating system. It demonstrates how to implement a Unix-like operating system using Rust's safety features while maintaining low-level control necessary for OS development.

## Project Structure

The project consists of three main components:

1. **kernel** - The OS kernel
   - Memory management (kalloc.rs)
   - Process management (proc.rs, process.rs)
   - File system (fs.rs, file.rs)
   - Device drivers (uart.rs, e1000.rs, virtio.rs)
   - Network stack (net/ directory)
   - CPU and interrupt handling (cpu.rs, trap.rs)
   - Low-level assembly code (entry.S, kernelvec.S, swtch.S, trampoline.S)

2. **user** - User programs
   - Basic Unix commands (cat, echo, ls, etc.)
   - System call interface (syscall.rs)
   - Test programs

3. **mkfs** - Filesystem creation tool
   - Tool to create the filesystem image

## Technical Features

- **Rust for bare-metal programming**: Using `#![no_std]` environment, custom memory allocators, and low-level hardware manipulation
- **RISC-V architecture**: Targeting the RISC-V 64-bit architecture
- **Multi-core support**: Support for multiple CPU cores (harts)
- **Networking**: Basic networking capabilities including a TCP/IP stack
- **Testing infrastructure**: Built-in testing for both kernel and user programs

## Building and Running

### Prerequisites

- Rust nightly toolchain with RISC-V target support
- QEMU with RISC-V support

### Setup

1. Install the Rust nightly toolchain:

```sh
rustup toolchain install nightly
```

2. Add the RISC-V target to your nightly Rust toolchain:

```sh
rustup target add riscv64imac-unknown-none-elf --toolchain nightly
```

> **Note**: This project requires the nightly Rust toolchain because it uses unstable features like `custom_test_frameworks`, `alloc_error_handler`, and `allocator_api`.
>
> **Important**: The RISC-V assembly code in this project requires the `zicsr` and `zifencei` extensions. These extensions are needed for instructions like `csrr` which are used in the assembly code. The build system has been configured to include these extensions.

### Build

```sh
make build
```

### Run in QEMU

```sh
make qemu
```

### Run Tests

```sh
# Run tests in debug mode (may encounter issues)
make test

# Run tests in release mode (recommended)
CARGO_RELEASE=1 make test
```

> **Important**: It is recommended to run tests in release mode using the `CARGO_RELEASE=1` flag. 
> The release mode compilation helps avoid certain issues that may occur in debug mode:
>
> 1. **Compiler Optimizations**: Release mode enables various optimizations that can prevent certain runtime issues, particularly in low-level code that interacts directly with hardware.
>
> 2. **Memory Layout**: Debug builds include additional information and different memory layouts that can sometimes trigger alignment issues or other memory-related problems in bare-metal environments.
>
> 3. **Inlining and Code Generation**: Release mode's aggressive inlining and code generation can avoid certain edge cases in function calls, especially in concurrent or interrupt-driven code.
>
> 4. **Performance**: Tests run significantly faster in release mode, which is beneficial for the more extensive test suites.

The `make test` command performs the following steps:

1. **Builds the mkfs tool and user programs**
   - Creates necessary binaries for the filesystem tool and user applications

2. **Builds the test harness for user libraries**
   - Runs `cargo test` with the `--no-run` flag for the user component
   - Creates symbolic links to the test executables

3. **Creates a test filesystem image**
   - Uses the mkfs tool to create a special filesystem image (fs.test.img)
   - Includes the test harness and user programs in this image

4. **Builds the test harness for the kernel library**
   - Compiles the kernel tests using `cargo test` with the `--no-run` flag

5. **Executes tests in QEMU**
   - Runs QEMU with the test filesystem image
   - Boots the kernel test harness
   - Tests run inside the emulated RISC-V environment

## System Calls

The OS implements standard Unix system calls:

- Process management: fork, exit, wait, exec
- File operations: open, read, write, close, unlink, mkdir, chdir, fstat
- Network operations: socket, bind, connect
- Memory management: 
  - [mmap](docs/mmap_implementation.md) - Maps files or devices into memory using lazy loading
  - [sbrk](docs/userland_memory_allocation.md) - Allocates memory for userland programs, enabling heap allocation

## User Program Implementation

### Structure of User Programs

User programs in xv6rs are implemented in Rust with the following characteristics:

1. **No Standard Library**: All programs use `#![no_std]` attribute since they run in a bare-metal environment without the Rust standard library.
2. **Entry Point**: Programs use the `entry_point!` macro which sets up the proper entry point and argument handling.
3. **System Call Interface**: Programs interact with the kernel through system calls defined in `user/src/syscall.rs`.
4. **Heap Allocation Support**: User programs can use heap allocation (like `Box`, `Vec`, etc.) through the custom global allocator implemented in the user environment.

### Basic Structure of a User Program

```rust
#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(xv6rs_user::test_runner)]
#![reexport_test_harness_main = "test_main"]

use xv6rs_user::{
    entry_point,
    syscall::{sys_write, sys_read}, // Import needed syscalls
    Args,
};

entry_point!(main);

fn main(args: &mut Args) -> Result<i32, &'static str> {
    // Program logic here
    // Return 0 for success or an error message
    Ok(0)
}
```

### Shell Implementation

The shell (`sh.rs`) is implemented with the following features:

1. **Command Execution**: Basic command execution using `fork` and `exec` system calls
2. **Pipes**: Support for command piping with the `|` operator
3. **Redirection**: Input (`<`) and output (`>`) redirection
4. **Background Execution**: Running commands in the background with `&`
5. **Built-in Commands**: Support for built-in commands like `cd` and `exit`

The shell implementation avoids heap allocation by using a more direct approach:
- For simple commands, it parses and executes them directly
- For pipes, it splits the input buffer and processes each part separately
- For background execution, it sets a flag to avoid waiting for the child process

### Example Shell Commands

```
$ ls                    # List files
$ cat file.txt          # Display file contents
$ echo hello > file.txt # Write to a file
$ cat < file.txt        # Read from a file
$ ls | grep txt         # Pipe output of ls to grep
$ sleep 10 &            # Run sleep in the background
$ cd /                  # Change directory
$ exit                  # Exit the shell
```

### Building User Programs

User programs are built as part of the main build process:

```sh
make build
```

This compiles all Rust files in `user/src/bin/` directory and includes them in the filesystem image.
