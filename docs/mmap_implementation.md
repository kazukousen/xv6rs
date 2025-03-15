# mmap System Call Implementation

This document explains the implementation and behavior of the mmap system call in xv6rs.

## Overview

The mmap system call maps files or devices into the virtual address space of a process. Key features include:

- Direct mapping of file contents into a process's memory space
- Lazy loading mechanism
- Potential use as shared memory

## Implementation Details

### System Call Interface

In `user/src/syscall.rs`, the user-side mmap system call interface is defined as:

```rust
pub fn sys_mmap(
    addr: *const u8,
    size: usize,
    prot: usize,
    flags: usize,
    fd: i32,
    offset: usize,
) -> usize
```

Parameters:
- `addr`: Virtual address for mapping (typically NULL, allowing the kernel to choose an appropriate address)
- `size`: Size of the mapping
- `prot`: Memory protection flags (READ, WRITE, EXEC)
- `flags`: Mapping type (SHARED, PRIVATE, ANONYMOUS)
- `fd`: File descriptor of the file to map (-1 for anonymous mapping)
- `offset`: Offset within the file

Return value:
- The mapped virtual address, or 0xffff_ffff_ffff_ffff if it fails

### Kernel-side Implementation

The kernel-side implementation is in the `sys_mmap` function in `kernel/src/proc/syscall.rs`:

```rust
/// 27
/// void *mmap(void *addr, size_t length, int prot, int flags, int fd, off_t offset)
/// returns that address, or 0xffff_ffff_ffff_ffff if it fails.
///
/// A file mapping maps a region of a file directly into the calling process's virtual memory.
/// Once a file is mapped, its contents can be accessed by operations on the bytes in the
/// corresponding memory region.
fn sys_mmap(&mut self) -> SysResult {
    // args
    // arg 0 `addr`
    let size = self.arg_i32(1)? as usize;
    let prot = self.arg_i32(2)? as usize;
    let prot = PteFlag::from_bits(prot).ok_or("sys_mmap: cannot parse prot")?;
    let flags = self.arg_i32(3)? as usize;
    let flags = MapFlag::from_bits(flags).ok_or("sys_mmap: cannot parse flags")?;
    let fd = self.arg_i32(4)?;

    let pdata = unsafe { &mut *self.data.get() };

    if fd != -1 {
        let f = pdata.o_files[fd as usize]
            .as_ref()
            .ok_or("sys_mmap: file not found")?;

        if (PteFlag::WRITE.bits() & prot.bits() > 0) && !f.writable {
            return Err("sys_mmap: file is read-only, but mmap has write permission and flag");
        }
    }

    let addr_end = pdata.cur_max;
    let addr_start = align_down(addr_end - size, PAGESIZE);

    pdata
        .vm_area
        .iter_mut()
        .find(|vm| {
            return vm.is_none();
        })
        .ok_or("cannot find unused vma")?
        .replace(VMA {
            addr_start,
            addr_end,
            size,
            prot,
            flags,
            fd,
        });
    pdata.cur_max = addr_start;

    return Ok(addr_start);
}
```

This function performs the following tasks:

1. Validates parameters passed from the user
2. Checks file permissions if a file is specified
3. Creates a Virtual Memory Area (VMA) entry
4. Allocates a virtual address

Importantly, this function **does not allocate physical memory or read the file**. As the comment states:

```rust
/// This syscall func does not allocate physical memory or read the file, just add new VMA
/// entry. Instead, do that in page fault handler.
```

This is because mmap adopts a "lazy loading" approach.

### Virtual Memory Area (VMA) Management

In `kernel/src/proc.rs`, each process maintains an array of Virtual Memory Areas (VMAs):

```rust
pub struct ProcData {
    // ...
    vm_area: [Option<VMA>; 100],
    cur_max: usize,
    // ...
}
```

Each VMA contains the following information:

```rust
struct VMA {
    addr_start: usize,
    addr_end: usize,
    size: usize,
    prot: PteFlag,
    flags: MapFlag,
    fd: i32,
}
```

`MapFlag` is defined as:

```rust
bitflags! {
    pub struct MapFlag: usize {
        const SHARED = 1 << 0;
        const PRIVATE = 1 << 1;
        const ANONYMOUNS = 1 << 2;
    }
}
```

### Lazy Loading Implementation

The most important feature of mmap is its lazy loading approach, which provides:

1. Fast mmap calls even for large files
2. Ability to map files larger than physical memory

The actual memory allocation and file reading occur in the page fault handler (`lazy_mmap` function):

```rust
pub fn lazy_mmap(&mut self, fault_addr: usize) -> Result<(), &'static str> {
    // find which VMA owns the VA.
    let vm = self
        .vm_area
        .iter()
        .find(|vm| {
            if vm.is_none() {
                return false;
            }
            let vm = vm.as_ref().unwrap();
            return vm.addr_start <= fault_addr && fault_addr <= vm.addr_end;
        })
        .ok_or("lazy_mmap: the addr is not lived in VMA")?
        .as_ref()
        .unwrap();

    let fault_addr_head = align_down(fault_addr, PAGESIZE);

    // map the page into the user address space, by installing to user page table.
    let pgt = self.page_table.as_mut().unwrap();
    // TODO: the physical page can be shared with mappings in other processes.
    // we will need reference counts on physical pages.
    // Right now, can only allocate a new physical page for each process.
    let pa = unsafe {
        SinglePage::alloc_into_raw()
            .expect("lazy_mmap: unable to allocate a page of physical memory")
    } as usize;
    pgt.map_pages(
        fault_addr_head,
        pa,
        PAGESIZE,
        //  vm.prot | PteFlag::USER,
        PteFlag::READ | PteFlag::WRITE | PteFlag::EXEC | PteFlag::USER,
    )?;

    if vm.fd < 0 && (MapFlag::ANONYMOUNS.bits() & vm.flags.bits()) > 0 {
        // anonymous mapping
        return Ok(());
    } else if vm.fd < 0 {
    }

    // TODO: even if the data is in kernel memory in the buffer cache, the current solution is
    // allocating a new physical page for each page read from mmap-ed file.
    // So try to modify this implementation to use that kernel memory, instead of allocating a
    // new page. This requires that file blocks be the same size as pages (set BSIZE to 4096).
    // and need to pin mmap-ed blocks into the buffer cache. We will need worry about reference
    // counts.
    //
    //
    // read 4096 bytes from the file to the page.
    let f = self.o_files[vm.fd as usize].as_ref().unwrap().clone();
    let offset = fault_addr_head - vm.addr_start;
    f.seek(offset);
    f.read(fault_addr_head, PAGESIZE)?;

    Ok(())
}
```

This function:

1. Finds the VMA entry containing the specified address
2. Allocates a physical page
3. Maps it in the page table
4. Returns if it's an anonymous mapping
5. Otherwise, reads data from the file

### Integration with Page Fault Handler

The page fault handler in `kernel/src/trap.rs` calls this function:

```rust
unsafe fn handle_trap(is_user: bool) {
    let scause = register::scause::get_type();
    match scause {
        // ...
        ScauseType::ExcPageLoad | ScauseType::ExcPageStoreAtomic => {
            if is_user {
                let fault_addr = register::stval::read();
                if let Err(e) = CPU_TABLE.my_proc().data.get_mut().lazy_mmap(fault_addr) {
                    panic!(
                        "handle_trap: failed to lazy allocate. {}. scause {:?} stval {:#x}",
                        e, scause, fault_addr
                    );
                }
            }
        }
        // ...
    }
}
```

When a user process tries to access mapped memory, a page fault occurs because the page hasn't been allocated to physical memory yet. The trap handler catches this page fault and calls the `lazy_mmap` function to perform the actual memory allocation and file reading.

### Usage Example

`user/src/bin/mmaptest.rs` contains an example of mmap usage:

```rust
fn main(_: &mut Args) -> Result<i32, &'static str> {
    let f = "mmaptest.tmp\0";
    make_file(f);
    let fd = sys_open(f, O_RDWR);
    if fd < 0 {
        return Err("open");
    }

    cat(fd)?;
    println!();

    let size = PAGESIZE + PAGESIZE;
    let buf = sys_mmap(ptr::null(), size, 1 << 1 | 1 << 2, 1 << 2, fd, 0);
    println!("mmap created!");
    let buf = unsafe { from_raw_parts(buf as *const u8, size) };
    println!("buf[0] {}", buf[0]);
    println!("buf[1] {}", buf[1]);

    println!("verify content");
    vaild_content(buf)?;

    Ok(0)
}
```

This test:

1. Creates a test file (1.5 pages of 'A's and 0.5 pages of zeros)
2. Opens the file
3. Displays the file contents
4. Maps the file into memory using mmap
5. Accesses the mapped memory contents
6. Verifies the contents

## Operational Flow

The operational flow of the mmap system call is as follows:

1. User program calls `sys_mmap`
2. Kernel creates a Virtual Memory Area (VMA) entry and returns a virtual address
3. At this point, no physical memory is allocated and no file is read
4. When the user program accesses that address, a page fault occurs
5. The page fault handler calls `lazy_mmap`, which allocates a physical page
6. If necessary, data is read from the file and mapped into the page table
7. User program execution resumes, and the mapped memory becomes accessible

This lazy loading approach allows mmap to efficiently handle large files.

## Memory Deallocation

To deallocate mapped memory, the `unmmap` function is used:

```rust
pub fn unmmap(&mut self, addr: usize, size: usize) -> Result<(), &'static str> {
    // find which VMA owns the VA.
    let mut vm = self
        .vm_area
        .iter_mut()
        .find(|vm| {
            if vm.is_none() {
                return false;
            }
            let vm = vm.as_ref().unwrap();
            return vm.addr_start <= addr && addr <= vm.addr_end;
        })
        .ok_or("lazy_mmap: the addr is not lived in VMA")?
        .as_mut();

    let addr_head = align_down(addr, PAGESIZE);
    let pgt = self.page_table.as_mut().unwrap();
    for va in (addr_head..=align_down(addr + size, PAGESIZE)).step_by(PAGESIZE) {
        if let Ok(_) = pgt.walk_addr(va) {
            pgt.unmap_pages(va, 1, true)?;
        }
    }

    self.cur_max = vm.as_ref().unwrap().addr_end;
    vm.take();

    Ok(())
}
```

This function:

1. Finds the VMA entry containing the specified address
2. Deallocates the mapped pages
3. Removes the VMA entry

## Optimization and Future Improvements

The code comments mention potential future optimizations:

```rust
// TODO: even if the data is in kernel memory in the buffer cache, the current solution is
// allocating a new physical page for each page read from mmap-ed file.
// So try to modify this implementation to use that kernel memory, instead of allocating a
// new page. This requires that file blocks be the same size as pages (set BSIZE to 4096).
// and need to pin mmap-ed blocks into the buffer cache. We will need worry about reference
// counts.
```

In the current implementation, a new physical page is allocated for each page of the file, but future optimizations could include:

1. Utilizing the buffer cache to reuse data already in kernel memory
2. Implementing reference counting to share physical pages between multiple processes
3. Matching file block size to page size (BSIZE = 4096)

These optimizations would improve memory usage efficiency and performance when multiple processes map the same file.
