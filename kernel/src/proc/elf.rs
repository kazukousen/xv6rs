use core::{cmp, mem};

use alloc::boxed::Box;

use crate::{
    fs::{InodeData, INODE_TABLE},
    log::LOG,
    page_table::PageTable,
    param::PAGESIZE,
    proc::ProcData,
    sleeplock::SleepLockGuard,
};

use super::{MAXARG, MAXARGLEN};

const MAGIC: u32 = 0x464C457F;

pub fn load(
    p: &mut ProcData,
    path: &[u8],
    argv: &[Option<Box<[u8; MAXARGLEN]>>; MAXARG],
) -> Result<usize, &'static str> {
    LOG.begin_op();

    let inode = match INODE_TABLE.namei(&path) {
        None => {
            LOG.end_op();
            return Err("cannot find inode by given path");
        }
        Some(inode) => inode,
    };

    let mut idata = inode.ilock();

    // read elf header
    let mut elfhdr = mem::MaybeUninit::<ELFHeader>::uninit();
    let elfhdr_ptr = elfhdr.as_mut_ptr() as *mut u8;
    match idata.readi(false, elfhdr_ptr, 0, mem::size_of::<ELFHeader>()) {
        Err(_) => {
            drop(idata);
            drop(inode);
            LOG.end_op();
            return Err("cannot read the elf file");
        }
        Ok(_) => {}
    }
    let elfhdr = unsafe { elfhdr.assume_init() };

    if elfhdr.magic != MAGIC {
        drop(idata);
        drop(inode);
        LOG.end_op();
        return Err("elf magic invalid");
    }

    let mut pgt = match PageTable::alloc_user_page_table(p.trapframe as usize) {
        None => {
            drop(idata);
            drop(inode);
            LOG.end_op();
            return Err("cannot alloc new user page table");
        }
        Some(pgt) => pgt,
    };

    let mut size = 0usize;

    // Load program into memory.
    let off_start = elfhdr.phoff as usize;
    let ph_size = mem::size_of::<ProgHeader>();
    let off_end = off_start + elfhdr.phnum as usize * ph_size;
    for off in (off_start..off_end).step_by(ph_size) {
        // read program header section
        let mut ph = mem::MaybeUninit::<ProgHeader>::uninit();
        let ph_ptr = ph.as_mut_ptr() as *mut u8;
        if idata.readi(false, ph_ptr, off, ph_size).is_err() {
            pgt.unmap_user_page_table(size);
            drop(idata);
            drop(inode);
            LOG.end_op();
            return Err("cannot read the program section");
        };
        let ph = unsafe { ph.assume_init() };

        size = match pgt.uvm_alloc(size, (ph.vaddr + ph.memsz) as usize) {
            Err(msg) => {
                pgt.unmap_user_page_table(size);
                drop(idata);
                drop(inode);
                LOG.end_op();
                return Err(msg);
            }
            Ok(size) => size,
        };

        if let Err(msg) = load_segment(
            &mut pgt,
            &mut idata,
            ph.vaddr as usize,
            ph.off as usize,
            ph.filesz as usize,
        ) {
            return Err(msg);
        };
    }

    drop(idata);
    drop(inode);
    LOG.end_op();

    size = align_up(size, PAGESIZE); // must be aligned

    // Allocate two pages.
    // Use the second as the user stack.
    size = match pgt.uvm_alloc(size, size + PAGESIZE * 2) {
        Err(msg) => {
            pgt.unmap_user_page_table(size);
            return Err(msg);
        }
        Ok(size) => size,
    };
    pgt.uvm_clear(size - 2 * PAGESIZE);
    let mut sp = size;
    let stackbase = sp - PAGESIZE;

    // Push argument strings, prepare rest of stack in ustack.
    let mut ustack: [usize; MAXARG + 1] = [0; MAXARG + 1];
    let mut argc = 0;
    for i in 0..MAXARG {
        let arg = argv[i].as_deref();
        if arg.is_none() {
            argc = i;
            break;
        }
        let arg = arg.unwrap();
        let count = arg.iter().position(|v| *v == 0).unwrap() + 1;
        sp -= count;
        sp -= sp % 16; // riscv sp must be 16-byte aligned.
        if sp < stackbase {
            pgt.unmap_user_page_table(size);
            return Err("pushing arguments causes stack over flow");
        }
        if let Err(msg) = pgt.copy_out(sp, arg.as_ptr(), count) {
            pgt.unmap_user_page_table(size);
            return Err(msg);
        };
        ustack[i] = sp;
    }
    // ustack[argc] = 0;

    // push the array of argv[] pointers.
    sp -= (argc + 1) * mem::size_of::<usize>();
    sp -= sp % 16;
    if sp < stackbase {
        pgt.unmap_user_page_table(size);
        return Err("pushing arguments causes stack over flow");
    }
    if let Err(msg) = pgt.copy_out(
        sp,
        ustack.as_ptr() as *const u8,
        (argc + 1) * mem::size_of::<usize>(),
    ) {
        pgt.unmap_user_page_table(size);
        return Err(msg);
    }

    // arguments to user main(argc, argv)
    let tf = unsafe { p.trapframe.as_mut().unwrap() };
    tf.a1 = sp;

    // comit to the user image
    let mut oldpgt = p.page_table.replace(pgt).unwrap();
    let oldsz = p.sz;
    p.sz = size;
    tf.epc = elfhdr.entry as usize;
    tf.sp = sp;
    oldpgt.unmap_user_page_table(oldsz);

    Ok(argc)
}

fn load_segment(
    pgt: &mut PageTable,
    idata: &mut SleepLockGuard<'_, InodeData>,
    va: usize,
    offset: usize,
    sz: usize,
) -> Result<(), &'static str> {
    for i in (0..sz).step_by(PAGESIZE) {
        let pa = pgt.walk_addr(va + i)?;
        let n = cmp::min(sz - i, PAGESIZE);
        if idata.readi(false, pa as *mut u8, offset + i, n).is_err() {
            return Err("load_segment: cannot read the program segment");
        };
    }

    Ok(())
}

/// File header
#[repr(C)]
struct ELFHeader {
    magic: u32,
    elf: [u8; 12],
    typed: u16,
    machine: u16,
    version: u32,
    entry: u64,
    // program header position
    phoff: u64,
    shoff: u64,
    flags: u32,
    ehsize: u16,
    phentsize: u16,
    // number of program headers
    phnum: u16,
    shentsize: u16,
    shnum: u16,
    shstrndx: u16,
}

/// Program section header
#[repr(C)]
struct ProgHeader {
    typed: u32,
    flags: u32,
    off: u64,
    vaddr: u64,
    paddr: u64,
    filesz: u64,
    memsz: u64,
    align: u64,
}

#[inline]
pub fn align_up(addr: usize, align: usize) -> usize {
    (addr + (align - 1)) & !(align - 1)
}
