pub const NCPU: usize = 8;

// qemu puts UART registers here in physical memory.
pub const UART0: usize = 0x1000_0000;

pub const QEMU_TEST0: usize = 0x100000;
pub const QEMU_EXIT_SUCCESS: u32 = 0x5555;
pub const QEMU_EXIT_FAIL: u32 = 0x13333; // exit 1
