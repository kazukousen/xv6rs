pub const NCPU: usize = 8;

// qemu puts UART registers here in physical memory.
pub const UART0: usize = 0x1000_0000;

pub const QEMU_TEST0: usize = 0x100000;
pub const QEMU_EXIT_SUCCESS: u32 = 0x5555;
pub const QEMU_EXIT_FAIL: u32 = 0x13333; // exit 1

// the kernel expects there to be RAM
// for use by the kernel and user pages
// from physical address 0x80000000 to PHYSTOP.
pub const KERNBASE: usize = 0x8000_0000;
pub const PHYSTOP: usize = KERNBASE + 128 * 1024 * 1024;
pub const PAGESIZE: usize = 4096;
