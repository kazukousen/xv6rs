use core::arch::asm;

use crate::{
    page_table::{PageTable, PteFlag},
    param::{
        CLINT, CLINT_MAP_SIZE, E1000_REGS_ADDR, ECAM0, KERNBASE, PAGESIZE, PHYSTOP, PLIC,
        PLIC_MAP_SIZE, TRAMPOLINE, UART0, VIRTIO0,
    },
    register::satp,
};

static mut KERNEL_PAGE_TABLE: PageTable = PageTable::empty();

pub unsafe fn init_hart() {
    satp::write(KERNEL_PAGE_TABLE.as_satp());
    // Each RISC-V core caches page table entries in a TLB. so when our OS changes a page table,
    // it must tell the CPU to invalidate corresponding cached TLB entries.
    //
    // after reloading the satp register, we must execute sfence.vma instruction to flush the
    // current core's TLB.
    asm!("sfence.vma zero, zero");
}

pub unsafe fn init() {
    // uart registers
    kvm_map(UART0, UART0, PAGESIZE, PteFlag::READ | PteFlag::WRITE);

    // virtio registers
    kvm_map(VIRTIO0, VIRTIO0, PAGESIZE, PteFlag::READ | PteFlag::WRITE);

    // PCI-E ECAM (configuration space) for e1000
    kvm_map(ECAM0, ECAM0, 0x1000_0000, PteFlag::READ | PteFlag::WRITE);

    // PCI-E MMIO for e1000
    kvm_map(
        E1000_REGS_ADDR as usize,
        E1000_REGS_ADDR as usize,
        0x2_0000,
        PteFlag::READ | PteFlag::WRITE,
    );

    // CLINT
    kvm_map(CLINT, CLINT, CLINT_MAP_SIZE, PteFlag::READ | PteFlag::WRITE);

    // PLIC
    kvm_map(PLIC, PLIC, PLIC_MAP_SIZE, PteFlag::READ | PteFlag::WRITE);

    // for TEST
    #[cfg(test)]
    {
        use crate::param::QEMU_TEST0;
        kvm_map(
            QEMU_TEST0,
            QEMU_TEST0,
            PAGESIZE,
            PteFlag::READ | PteFlag::WRITE,
        );
    }

    extern "C" {
        fn _etext(); // see kernel.ld linker script
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

    extern "C" {
        fn trampoline();
    }

    kvm_map(
        TRAMPOLINE,
        trampoline as usize,
        PAGESIZE,
        PteFlag::READ | PteFlag::EXEC,
    );
}

pub unsafe fn kvm_map(va: usize, pa: usize, size: usize, perm: PteFlag) {
    if let Err(err) = KERNEL_PAGE_TABLE.map_pages(va, pa, size, perm) {
        panic!("kvm_map: {}", err)
    }
}
