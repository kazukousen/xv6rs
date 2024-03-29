use core::{cmp, mem};

use alloc::boxed::Box;

use crate::{
    fs::{InodeData, INODE_TABLE},
    log::LOG,
    page_table::{align_up, PageTable},
    param::PAGESIZE,
    proc::ProcData,
    sleeplock::SleepLockGuard,
};

use super::{MAXARG, MAXARGLEN};

const MAGIC: u32 = 0x464C457F;
const PROG_LOAD: u32 = 1;

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

    // Allocate a new user page table with 2 pages (trampoline and trapframe).
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
        if ph.typ != PROG_LOAD {
            continue;
        }
        if ph.memsz == 0 {
            continue;
        }

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

        if ph.vaddr as usize % PAGESIZE != 0 {
            pgt.unmap_user_page_table(size);
            drop(idata);
            drop(inode);
            LOG.end_op();
            return Err("program header vaddr not aligned page size");
        }

        if let Err(msg) = load_segment(
            &mut pgt,
            &mut idata,
            ph.vaddr as usize,
            ph.off as usize,
            ph.filesz as usize,
        ) {
            pgt.unmap_user_page_table(size);
            drop(idata);
            drop(inode);
            LOG.end_op();
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

    // the arguments laid out ...
    //
    //  | &argv[0] | &argv[1] | ... | 0 | argv[n][0] | ... | argv[1][m] | argv[0][0] | argv[0][m] | ...
    // ^ sp, and the second argument (`a1` register)

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
        let arg_size = arg.iter().position(|v| *v == 0).unwrap() + 1;
        sp -= arg_size;
        sp -= sp % 16; // riscv sp must be 16-byte aligned.
        if sp < stackbase {
            pgt.unmap_user_page_table(size);
            return Err("pushing arguments causes stack over flow");
        }
        // copy out argv[i]'s data to the virtual address pointed to by `sp`.
        if let Err(msg) = pgt.copy_out(sp, arg.as_ptr(), arg_size) {
            pgt.unmap_user_page_table(size);
            return Err(msg);
        };
        ustack[i] = sp;
    }

    // push the array of argv[] pointers.
    let ustack_size = (argc + 1) * mem::size_of::<usize>(); // ustack[argc] = 0;
    sp -= ustack_size;
    sp -= sp % 16;
    if sp < stackbase {
        pgt.unmap_user_page_table(size);
        return Err("pushing arguments causes stack over flow");
    }
    if let Err(msg) = pgt.copy_out(sp, ustack.as_ptr() as *const u8, ustack_size) {
        pgt.unmap_user_page_table(size);
        return Err(msg);
    }

    // arguments to user main(argc, argv)
    let tf = unsafe { p.trapframe.as_mut().unwrap() };
    // pass the pointer of the array of argv[] pointers as the second argument in user space
    tf.a1 = sp;

    // comit to the user image
    let mut oldpgt = p.page_table.replace(pgt).unwrap();
    let oldsz = p.sz;
    p.sz = size;
    tf.epc = elfhdr.entry as usize;
    tf.sp = sp;
    oldpgt.unmap_user_page_table(oldsz);

    // pass the `argc` as the first argument in user space
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
#[derive(Debug)]
#[repr(C)]
struct ELFHeader {
    magic: u32,
    bit_size: u8, // This byte is set to either `1` or `2` to signify 32- or 64-bit format, respectively.
    endian: u8, // This byte is set to either `1` or `2` to littler or big endianness, respectively.
    elf_version: u8, // Set 1 for the original and current version of ELF.
    os_abi: u8, // Identifies the target operating system ABI.
    abi_version: u8,
    padding: [u8; 7], // reserved padding bytes, currently unused.
    typ: u16,
    machine: u16,
    version: u32,
    entry: u64, // the memory address of the entry point from where the process starts executing.
    phoff: u64, // points to the start of the program header table.
    shoff: u64, // points to the start of the section header table.
    flags: u32,
    ehsize: u16,
    phentsize: u16,
    phnum: u16, // number of program headers
    shentsize: u16,
    shnum: u16,
    shstrndx: u16,
}

/// Program section header
#[derive(Debug)]
#[repr(C)]
struct ProgHeader {
    typ: u32,
    flags: u32,
    off: u64,
    vaddr: u64,
    paddr: u64,
    filesz: u64,
    memsz: u64,
    align: u64,
}
