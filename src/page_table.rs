use core::{
    alloc::AllocError,
    ops::{Index, IndexMut},
    ptr,
};

use alloc::boxed::Box;
use bitflags::bitflags;

use crate::param::{PAGESIZE, TRAMPOLINE, TRAPFRAME};

bitflags! {
    pub struct PteFlag: usize {
        const VALID = 1 << 0;
        const READ = 1 << 1;
        const WRITE = 1 << 2;
        const EXEC = 1 << 3;
        const USER = 1 << 4;
        const GLOB = 1 << 5;
        const ACCES = 1 << 6;
        const DIRTY = 1 << 7;
    }
}

pub trait Page: Sized {
    unsafe fn alloc_into_raw() -> Result<*mut Self, AllocError> {
        let page = Box::<Self>::try_new_zeroed()?.assume_init();
        Ok(Box::into_raw(page))
    }

    unsafe fn free_from_raw(raw: *mut Self) {
        drop(Box::from_raw(raw))
    }
}

#[repr(C, align(4096))]
pub struct SinglePage {
    data: [u8; PAGESIZE],
}

impl Page for SinglePage {}

#[repr(C, align(4096))]
pub struct PageTable {
    entries: [PageTableEntry; 512],
}

impl Page for PageTable {}

impl PageTable {
    pub const fn empty() -> Self {
        const EMPTY: PageTableEntry = PageTableEntry::new();
        Self {
            entries: [EMPTY; 512],
        }
    }

    pub fn as_satp(&self) -> usize {
        (8 << 60) | ((self as *const PageTable as usize) >> 12)
    }

    /// Allocate a new user page table.
    pub fn alloc_user_page_table(trapframe: usize) -> Option<Box<Self>> {
        extern "C" {
            fn trampoline(); // in trampoline.S
        }
        let mut pgt = unsafe { Box::<Self>::try_new_zeroed().ok()?.assume_init() };

        pgt.map_pages(
            TRAMPOLINE,
            trampoline as usize,
            PAGESIZE,
            PteFlag::READ | PteFlag::EXEC,
        )
        .ok()?;

        pgt.map_pages(
            TRAPFRAME,
            trapframe,
            PAGESIZE,
            PteFlag::READ | PteFlag::WRITE,
        )
        .ok()?;

        Some(pgt)
    }

    /// Load the user initcode into address 0 of pagetable,
    /// for the very first process.
    /// sz must be less than a page.
    pub fn uvm_init(&mut self, code: &[u8]) -> Result<(), &'static str> {
        if code.len() >= PAGESIZE {
            return Err("uvm_init: more than a page");
        }

        let mem = unsafe {
            SinglePage::alloc_into_raw().or_else(|_| Err("uvm_init: insufficient memory"))?
        };
        self.map_pages(
            0,
            mem as usize,
            PAGESIZE,
            PteFlag::READ | PteFlag::WRITE | PteFlag::EXEC | PteFlag::USER,
        )?;

        // copy the code
        unsafe {
            ptr::copy_nonoverlapping(code.as_ptr(), mem as *mut u8, code.len());
        }

        Ok(())
    }

    pub fn map_pages(
        &mut self,
        va: usize,
        pa: usize,
        size: usize,
        perm: PteFlag,
    ) -> Result<(), &'static str> {
        let va_start = align_down(va, PAGESIZE);
        let va_end = align_up(va + size, PAGESIZE);

        let mut pa = pa;

        for va in (va_start..va_end).step_by(PAGESIZE) {
            match self.walk_alloc(va) {
                Some(pte) => {
                    if pte.is_valid() {
                        return Err("map_pages: remap");
                    } else {
                        pte.set_addr(as_pte_addr(pa), perm);
                    }
                }
                None => {
                    return Err("map_pages: not enough memory for new page table");
                }
            }

            pa += PAGESIZE;
        }

        Ok(())
    }

    fn unmap_pages(
        &mut self,
        va_start: usize,
        n: usize,
        freeing: bool,
    ) -> Result<(), &'static str> {
        if va_start % PAGESIZE != 0 {
            panic!("unmap_pages: not aligned");
        }

        for va in (va_start..(va_start + n * PAGESIZE)).step_by(PAGESIZE) {
            match self.walk_mut(va) {
                Some(pte) => {
                    if !pte.is_valid() {
                        return Err("not mapped");
                    }
                    if !pte.is_leaf() {
                        return Err("not a leaf");
                    }
                    if freeing {
                        let pa = pte.as_phys_addr();
                        unsafe { SinglePage::free_from_raw(pa as *mut SinglePage) };
                    }
                    pte.data = 0;
                }
                None => {
                    return Err("unmap_pages: pte not found");
                }
            }
        }

        Ok(())
    }

    fn walk_alloc(&mut self, va: usize) -> Option<&mut PageTableEntry> {
        let mut page_table = self as *mut PageTable;

        for level in (1..=2).rev() {
            let pte = unsafe { &mut page_table.as_mut().unwrap()[get_index(va, level)] };

            if !pte.is_valid() {
                // The raw page_table pointer is leaked but kept in the page table entry that can calculate later.
                let page_table_ptr = unsafe { PageTable::alloc_into_raw().ok()? };

                pte.set_addr(as_pte_addr(page_table_ptr as usize), PteFlag::VALID);
            }

            page_table = pte.as_page_table();
        }

        unsafe { Some(&mut page_table.as_mut().unwrap()[get_index(va, 0)]) }
    }

    fn walk(&self, va: usize) -> Option<&PageTableEntry> {
        let mut page_table = self as *const PageTable;

        for level in (1..=2).rev() {
            let pte = unsafe { &page_table.as_ref().unwrap()[get_index(va, level)] };

            if !pte.is_valid() {
                return None;
            }

            page_table = pte.as_page_table();
        }

        unsafe { Some(&page_table.as_ref().unwrap()[get_index(va, 0)]) }
    }

    fn walk_mut(&mut self, va: usize) -> Option<&mut PageTableEntry> {
        let mut page_table = self as *mut PageTable;

        for level in (1..=2).rev() {
            let pte = unsafe { &page_table.as_ref().unwrap()[get_index(va, level)] };

            if !pte.is_valid() {
                return None;
            }

            page_table = pte.as_page_table();
        }

        unsafe { Some(&mut page_table.as_mut().unwrap()[get_index(va, 0)]) }
    }
}

impl Drop for PageTable {
    fn drop(&mut self) {
        self.entries.iter_mut().for_each(|e| e.free());
    }
}

#[inline]
fn align_down(addr: usize, align: usize) -> usize {
    assert!(align.is_power_of_two());
    addr & !(align - 1)
}

#[inline]
fn align_up(addr: usize, align: usize) -> usize {
    assert!(align.is_power_of_two());
    (addr + align - 1) & !(align - 1)
}

fn get_index(va: usize, level: usize) -> PageTableIndex {
    PageTableIndex(((va >> (12 + level * 9)) & 0x1FF) as u16)
}

fn as_pte_addr(pa: usize) -> usize {
    (pa >> 12) << 10
}

impl Index<PageTableIndex> for PageTable {
    type Output = PageTableEntry;

    #[inline]
    fn index(&self, index: PageTableIndex) -> &Self::Output {
        &self.entries[usize::from(index.0)]
    }
}

impl IndexMut<PageTableIndex> for PageTable {
    #[inline]
    fn index_mut(&mut self, index: PageTableIndex) -> &mut Self::Output {
        &mut self.entries[usize::from(index.0)]
    }
}

/// A 9-bits index for page table.
struct PageTableIndex(u16);

#[derive(Debug)]
#[repr(C)]
struct PageTableEntry {
    data: usize, // Physical Page Number (44 bit) + Flags (10 bit)
}

impl PageTableEntry {
    #[inline]
    const fn new() -> Self {
        Self { data: 0 }
    }

    #[inline]
    fn is_valid(&self) -> bool {
        (self.data & PteFlag::VALID.bits()) > 0
    }

    #[inline]
    fn is_leaf(&self) -> bool {
        (self.data & (PteFlag::READ | PteFlag::WRITE | PteFlag::EXEC).bits()) > 0
    }

    #[inline]
    fn set_addr(&mut self, addr: usize, perm: PteFlag) {
        self.data = addr | (perm | PteFlag::VALID).bits();
    }

    #[inline]
    fn as_page_table(&self) -> *mut PageTable {
        // Physical Page Number (44 bit) + Offset (12 bit)
        (self.data >> 10 << 12) as *mut PageTable
    }

    #[inline]
    fn as_phys_addr(&self) -> usize {
        // Physical Page Number (44 bit) + Offset (12 bit)
        self.data >> 10 << 12
    }

    fn free(&mut self) {
        if self.is_valid() {
            if self.is_leaf() {
                // phys memory should already be freed.
                panic!("freeing a PTE leaf")
            }
            unsafe { PageTable::free_from_raw(self.as_page_table()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::param::KERNBASE;

    use super::*;

    #[test_case]
    fn map_unmap_pages() {
        let pgt = Box::<PageTable>::try_new_zeroed();
        assert!(pgt.is_ok());
        let mut pgt = unsafe { pgt.unwrap().assume_init() };

        extern "C" {
            fn _etext(); // see kernel.ld linker script
        }
        let etext = _etext as usize;

        // map kernel text executable and read-only.
        pgt.map_pages(
            KERNBASE,
            KERNBASE,
            etext - KERNBASE,
            PteFlag::READ | PteFlag::EXEC,
        )
        .expect("map_pages");

        let pte = pgt.walk(KERNBASE).expect("walk");
        assert_eq!(KERNBASE, pte.as_phys_addr());

        pgt.unmap_pages(KERNBASE, (etext - KERNBASE) / PAGESIZE, false)
            .expect("unmap_pages");

        drop(pgt);
    }
}
