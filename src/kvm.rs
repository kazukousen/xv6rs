use core::arch::asm;

use crate::{
    page_table::{PageTable, PteFlag},
    param::{KERNBASE, PAGESIZE, PHYSTOP, QEMU_TEST0, UART0},
    register::satp,
};

static mut KERNEL_PAGE_TABLE: PageTable = PageTable::empty();

pub unsafe fn init_hart() {
    satp::write(KERNEL_PAGE_TABLE.as_satp());
    asm!("sfence.vma zero, zero");
}

pub unsafe fn init() {
    // uart registers
    kvm_map(UART0, UART0, PAGESIZE, PteFlag::READ | PteFlag::WRITE);

    // for TEST
    #[cfg(test)]
    kvm_map(
        QEMU_TEST0,
        QEMU_TEST0,
        PAGESIZE,
        PteFlag::READ | PteFlag::WRITE,
    );

    extern "C" {
        fn _etext();
    }
    let etext = _etext as usize;

    // map kernel text executable and read-only.
    kvm_map(
        KERNBASE,
        KERNBASE,
        etext - KERNBASE,
        PteFlag::READ | PteFlag::EXEC,
    );

    // map kernel data and the physical RAM we'll make use of.
    kvm_map(
        etext,
        etext,
        PHYSTOP - etext,
        PteFlag::READ | PteFlag::WRITE,
    );
}

pub unsafe fn kvm_map(va: usize, pa: usize, size: usize, perm: PteFlag) {
    if let Err(err) = KERNEL_PAGE_TABLE.map_pages(va, pa, size, perm) {
        panic!("kvm_map: {}", err)
    }
}
