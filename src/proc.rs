use core::{cell::UnsafeCell, mem, ptr};

use alloc::{boxed::Box, sync::Arc};
use array_macro::array;

use crate::{
    cpu::{CpuTable, CPU_TABLE},
    file::File,
    fs::{self, Inode, INODE_TABLE},
    page_table::{Page, PageTable, SinglePage},
    param::{KSTACK_SIZE, NOFILE, PAGESIZE, ROOTDEV},
    println,
    process::PROCESS_TABLE,
    register::satp,
    spinlock::{SpinLock, SpinLockGuard},
    trap::{user_trap_ret, usertrap},
};

mod elf;
mod syscall;

use self::syscall::Syscall;

#[repr(C)]
pub struct Context {
    pub ra: usize,
    pub sp: usize,

    // callee saved
    s0: usize,
    s1: usize,
    s2: usize,
    s3: usize,
    s4: usize,
    s5: usize,
    s6: usize,
    s7: usize,
    s8: usize,
    s9: usize,
    s10: usize,
    s11: usize,
}

impl Context {
    pub const fn new() -> Self {
        Self {
            ra: 0,
            sp: 0,
            s0: 0,
            s1: 0,
            s2: 0,
            s3: 0,
            s4: 0,
            s5: 0,
            s6: 0,
            s7: 0,
            s8: 0,
            s9: 0,
            s10: 0,
            s11: 0,
        }
    }

    fn clear(&mut self) {
        self.ra = 0;
        self.sp = 0;
        self.s1 = 0;
        self.s2 = 0;
        self.s3 = 0;
        self.s4 = 0;
        self.s5 = 0;
        self.s6 = 0;
        self.s7 = 0;
        self.s8 = 0;
        self.s9 = 0;
        self.s10 = 0;
        self.s11 = 0;
    }
}

#[repr(C)]
pub struct TrapFrame {
    /* 0 */ kernel_satp: usize,
    /* 8 */ kernel_sp: usize,
    /* 16 */ kernel_trap: usize,
    /* 24 */ epc: usize,
    /* 32 */ kernel_hartid: usize,
    /* 40 */ ra: usize,
    /* 48 */ sp: usize,
    /* 56 */ gp: usize,
    /* 64 */ tp: usize,
    /*  72 */ t0: usize,
    /*  80 */ t1: usize,
    /*  88 */ t2: usize,
    /*  96 */ s0: usize,
    /* 104 */ s1: usize,
    /* 112 */ a0: usize,
    /* 120 */ a1: usize,
    /* 128 */ a2: usize,
    /* 136 */ a3: usize,
    /* 144 */ a4: usize,
    /* 152 */ a5: usize,
    /* 160 */ a6: usize,
    /* 168 */ a7: usize,
    /* 176 */ s2: usize,
    /* 184 */ s3: usize,
    /* 192 */ s4: usize,
    /* 200 */ s5: usize,
    /* 208 */ s6: usize,
    /* 216 */ s7: usize,
    /* 224 */ s8: usize,
    /* 232 */ s9: usize,
    /* 240 */ s10: usize,
    /* 248 */ s11: usize,
    /* 256 */ t3: usize,
    /* 264 */ t4: usize,
    /* 272 */ t5: usize,
    /* 280 */ t6: usize,
}

pub struct ProcInner {
    pub state: ProcState,
    // sleeping on channel
    pub chan: usize,
    pub pid: usize,
    pub killed: bool,
    pub exit_status: i32,
}

impl ProcInner {
    const fn new() -> Self {
        Self {
            state: ProcState::Unused,
            chan: 0,
            pid: 0,
            killed: false,
            exit_status: 0,
        }
    }
}

#[derive(PartialEq)]
pub enum ProcState {
    Unused,
    Allocated,
    Runnable,
    Running,
    Sleeping,
    Zombie,
}

pub struct ProcData {
    kstack: usize,
    sz: usize,
    page_table: Option<Box<PageTable>>,
    trapframe: *mut TrapFrame,
    context: Context,
    pub cwd: Option<Inode>,
    pub o_files: [Option<Arc<File>>; NOFILE],
}

impl ProcData {
    const fn new() -> Self {
        Self {
            kstack: 0,
            sz: 0,
            page_table: None,
            trapframe: ptr::null_mut(),
            context: Context::new(),
            cwd: None,
            o_files: array![_ => None; NOFILE],
        }
    }

    pub fn set_kstack(&mut self, v: usize) {
        self.kstack = v;
    }

    pub fn init_page_table(&mut self) -> Result<(), ()> {
        self.trapframe =
            unsafe { SinglePage::alloc_into_raw() }.or_else(|_| Err(()))? as *mut TrapFrame;

        let pgt = PageTable::alloc_user_page_table(self.trapframe as usize).ok_or_else(|| ())?;
        self.page_table = Some(pgt);
        Ok(())
    }

    pub fn init_context(&mut self) {
        self.context.clear();
        self.context.ra = forkret as usize;
        self.context.sp = self.kstack + KSTACK_SIZE;
    }

    /// initialize the user first process
    pub fn user_init(&mut self) -> Result<(), &'static str> {
        // allocate one user page and copy init's instructions
        // and data into it.
        self.page_table.as_mut().unwrap().uvm_init(&INITCODE)?;
        self.sz = PAGESIZE;

        // prepare for the very first "return" from kernel to user.
        let trapframe = unsafe { self.trapframe.as_mut().unwrap() };
        trapframe.epc = 0; // user program counter
        trapframe.sp = PAGESIZE; // user stack poiner

        self.cwd = Some(
            INODE_TABLE
                .namei(&[b'/', 0])
                .expect("cannot find root inode by b'/'"),
        );
        Ok(())
    }

    pub fn get_context(&mut self) -> *mut Context {
        &mut self.context as *mut _
    }

    #[inline]
    pub unsafe fn get_epc(&self) -> usize {
        self.trapframe.as_ref().unwrap().epc
    }

    #[inline]
    pub unsafe fn set_epc(&self, epc: usize) {
        self.trapframe.as_mut().unwrap().epc = epc;
    }

    pub unsafe fn setup_user_ret(&self) -> usize {
        let trapframe = self.trapframe.as_mut().unwrap();
        trapframe.kernel_satp = satp::read();
        trapframe.kernel_sp = self.kstack + KSTACK_SIZE;
        trapframe.kernel_trap = usertrap as usize;
        trapframe.kernel_hartid = CpuTable::cpu_id();

        self.page_table.as_ref().unwrap().as_satp()
    }

    #[inline]
    fn copy_in(&self, dst: *mut u8, srcva: usize, count: usize) -> Result<(), &'static str> {
        self.page_table.as_ref().unwrap().copy_in(dst, srcva, count)
    }

    #[inline]
    pub fn copy_out(&self, dstva: usize, src: *const u8, count: usize) -> Result<(), &'static str> {
        self.page_table
            .as_ref()
            .unwrap()
            .copy_out(dstva, src, count)
    }
}

pub struct Proc {
    pub index: usize,
    pub inner: SpinLock<ProcInner>,
    pub data: UnsafeCell<ProcData>,
}

impl Proc {
    pub const fn new(index: usize) -> Self {
        Self {
            index,
            inner: SpinLock::new(ProcInner::new(), "proc"),
            data: UnsafeCell::new(ProcData::new()),
        }
    }

    pub unsafe fn yield_process(&self) {
        let mut guard = self.inner.lock();
        if guard.state == ProcState::Running {
            let ctx = &(*self.data.get()).context;
            guard.state = ProcState::Runnable;
            guard = CPU_TABLE.my_cpu_mut().sched(guard, ctx);
        }
        drop(guard);
    }

    /// Atomically release lock and sleep on chan.
    /// The passed-in guard must not be the proc's guard to avoid deadlock.
    pub fn sleep<'a, T>(&self, chan: usize, lk: SpinLockGuard<'a, T>) -> SpinLockGuard<'a, T> {
        let mut guard = self.inner.lock();

        // Go to sleep
        guard.chan = chan;
        guard.state = ProcState::Sleeping;

        // unlock lk
        let weaked = lk.weak();

        unsafe {
            let cpu = CPU_TABLE.my_cpu_mut();
            guard = cpu.sched(guard, &(*self.data.get()).context);
        }

        // Tidy up.
        guard.chan = 0;
        weaked.lock()
    }

    /// allocates the new process and gives it exactly the same memory contents as the calling
    /// process.
    /// this function returns the new process's pid in the calling process, and returns zero in the child process.
    pub fn fork(&mut self) -> Result<usize, &'static str> {
        let child =
            unsafe { PROCESS_TABLE.alloc_proc() }.ok_or_else(|| "cannot allocate new process")?;

        let mut cguard = child.inner.lock();

        // copy user memory from parent to child.
        let pdata = self.data.get_mut();
        let cdata = child.data.get_mut();
        let cpgt = cdata.page_table.as_mut().unwrap();
        let sz = pdata.sz;
        if pdata
            .page_table
            .as_mut()
            .unwrap()
            .uvm_copy(cpgt, sz)
            .is_err()
        {
            Self::free(cdata, &mut cguard);
            return Err("fork: cannot uvm_copy");
        };
        cdata.sz = sz;

        // copy saved user registers.
        unsafe { ptr::copy_nonoverlapping(pdata.trapframe, cdata.trapframe, 1) };

        // cause fork to return 0 in the child.
        unsafe { cdata.trapframe.as_mut() }.unwrap().a0 = 0;

        // incremenet reference counts on open file descriptors.
        for i in 0..pdata.o_files.len() {
            if let Some(ref f) = pdata.o_files[i] {
                cdata.o_files[i].replace(f.clone());
            }
        }
        cdata.cwd = Some(INODE_TABLE.idup(&pdata.cwd.as_ref().unwrap()));
        drop(cguard);

        // set parent
        let mut parents = unsafe { PROCESS_TABLE.parents.lock() };
        parents[child.index] = Some(self.index);
        drop(parents);

        let mut cguard = child.inner.lock();
        cguard.state = ProcState::Runnable;
        let pid = cguard.pid;
        drop(cguard);

        Ok(pid)
    }

    pub fn free(pdata: &mut ProcData, inner: &mut SpinLockGuard<ProcInner>) {
        if !pdata.trapframe.is_null() {
            unsafe { SinglePage::free_from_raw(pdata.trapframe as *mut _) };
            pdata.trapframe = ptr::null_mut();
        }
        if pdata.page_table.is_some() {
            pdata
                .page_table
                .as_mut()
                .unwrap()
                .unmap_user_page_table(pdata.sz);
            drop(pdata.page_table.take());
        }
        pdata.sz = 0;
        inner.state = ProcState::Unused;
        inner.chan = 0;
        inner.pid = 0;
        inner.killed = false;
        inner.exit_status = 0;
    }

    pub fn syscall(&mut self) {
        let trapframe = unsafe { self.data.get_mut().trapframe.as_mut() }.unwrap();

        // sepc points to the ecall instruction,
        // but we want to return to the next instruction.
        trapframe.epc += 4;

        let num = trapframe.a7;
        let ret = match num {
            1 => self.sys_fork(),
            2 => self.sys_exit(),
            3 => self.sys_wait(),
            5 => self.sys_read(),
            7 => self.sys_exec(),
            8 => self.sys_fstat(),
            10 => self.sys_dup(),
            12 => self.sys_sbrk(),
            15 => self.sys_open(),
            16 => self.sys_write(),
            21 => self.sys_close(),
            _ => {
                panic!("unknown syscall: {}", num);
            }
        };
        trapframe.a0 = match ret {
            Ok(ret) => ret,
            Err(msg) => {
                println!("syscall error: no={} {}", num, msg);
                -1isize as usize
            }
        };
    }

    #[inline]
    fn arg_str(&mut self, n: usize, dst: &mut [u8]) -> Result<usize, &'static str> {
        let addr = self.arg_raw(n)?;
        self.fetch_str(addr, dst)
    }

    #[inline]
    fn fetch_str(&mut self, addr: usize, dst: &mut [u8]) -> Result<usize, &'static str> {
        self.data
            .get_mut()
            .page_table
            .as_ref()
            .unwrap()
            .copy_in_str(dst, addr)
    }

    #[inline]
    fn arg_raw(&mut self, n: usize) -> Result<usize, &'static str> {
        let tf = unsafe { self.data.get_mut().trapframe.as_ref().unwrap() };
        match n {
            0 => Ok(tf.a0),
            1 => Ok(tf.a1),
            2 => Ok(tf.a2),
            3 => Ok(tf.a3),
            4 => Ok(tf.a4),
            5 => Ok(tf.a5),
            _ => Err("arg raw"),
        }
    }

    #[inline]
    fn arg_i32(&mut self, n: usize) -> Result<i32, &'static str> {
        let addr = self.arg_raw(n)?;
        Ok(addr as i32)
    }

    #[inline]
    fn arg_fd(&mut self, n: usize) -> Result<i32, &'static str> {
        let fd = self.arg_i32(n)?;
        if fd < 0 {
            return Err("file descriptor must be greater than or equal to 0");
        }
        if fd >= NOFILE.try_into().unwrap() {
            return Err("file descriptor must be less than NOFILE");
        }

        if self.data.get_mut().o_files[fd as usize].is_none() {
            return Err("file descriptor not allocated");
        }

        Ok(fd)
    }

    #[inline]
    fn alloc_fd(&mut self) -> Result<usize, ()> {
        for (i, f) in self.data.get_mut().o_files.iter().enumerate() {
            if f.is_none() {
                return Ok(i);
            }
        }
        Err(())
    }

    #[inline]
    fn fetch_addr(&mut self, addr: usize) -> Result<usize, &'static str> {
        if addr >= self.data.get_mut().sz || addr + mem::size_of::<usize>() > self.data.get_mut().sz
        {
            return Err("fetch_addr size");
        }
        let mut dst: usize = 0;
        self.data.get_mut().page_table.as_ref().unwrap().copy_in(
            &mut dst as *mut usize as *mut u8,
            addr,
            mem::size_of::<usize>(),
        )?;
        Ok(dst)
    }
}

pub fn either_copy_out(is_user: bool, dst: *mut u8, src: *const u8, count: usize) {
    if is_user {
        let p = unsafe { CPU_TABLE.my_proc() };
        p.data
            .get_mut()
            .copy_out(dst as usize, src, count)
            .expect("either_copy_out");
    } else {
        unsafe { ptr::copy(src, dst, count) };
    }
}

pub fn either_copy_in(is_user: bool, src: *const u8, dst: *mut u8, count: usize) {
    if is_user {
        let p = unsafe { CPU_TABLE.my_proc() };
        p.data
            .get_mut()
            .copy_in(dst, src as usize, count)
            .expect("either_copy_in");
    } else {
        unsafe { ptr::copy(src, dst, count) };
    }
}

static mut FIRST: bool = true;

/// an allocated process is switched to here by scheduler().
unsafe fn forkret() {
    CPU_TABLE.my_proc().inner.unlock();
    if FIRST {
        FIRST = false;
        fs::init(ROOTDEV);

        // entry point for `cargo test`
        #[cfg(test)]
        crate::test_main();
    }

    user_trap_ret();
}

/// first user program that calls exec("/init")
static INITCODE: [u8; 51] = [
    0x17, 0x05, 0x00, 0x00, 0x13, 0x05, 0x05, 0x02, 0x97, 0x05, 0x00, 0x00, 0x93, 0x85, 0x05, 0x02,
    0x9d, 0x48, 0x73, 0x00, 0x00, 0x00, 0x89, 0x48, 0x73, 0x00, 0x00, 0x00, 0xef, 0xf0, 0xbf, 0xff,
    0x2f, 0x69, 0x6e, 0x69, 0x74, 0x00, 0x00, 0x01, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_forkret() {
        let pdata = unsafe { CPU_TABLE.my_proc() }.data.get_mut();
        assert_eq!(PAGESIZE, pdata.sz);
        let tf = unsafe { pdata.trapframe.as_ref() }.unwrap();
        assert_eq!(PAGESIZE, tf.sp);
    }

    /*
     * immediately exit with status 42.
     *
     * od -t xC exit42code
     *
     * ```
     * #include "syscall.h"
     * .globl start
     * start:
     *   li a0, 42
     *   li a7, SYS_exit
     * ecall
     * ```
     */
    static EXIT_42_CODE: [u8; 12] = [
        0x13, 0x05, 0xa0, 0x02, 0x93, 0x08, 0x20, 0x00, 0x73, 0x00, 0x00, 0x00,
    ];

    #[test_case]
    fn test_fork_exit_wait_with_return_42_code() {
        let p = unsafe { CPU_TABLE.my_proc() };
        let pdata = p.data.get_mut();
        let pgt = pdata.page_table.as_mut().unwrap();

        // remap
        pgt.unmap_pages(0, 1, true).expect("cannot unmap initcode");
        pgt.uvm_init(&EXIT_42_CODE)
            .expect("cannot map the test code into the page");

        // the child process would be scheduled on cpu_id=1, then runs the code in user space,
        // exits with status 42.
        let child_pid = p.fork().expect("fork failed");
        assert_eq!(1, child_pid);

        let waited_pid = unsafe { PROCESS_TABLE.wait(p, 1) }.expect("wait failed");
        assert_eq!(child_pid, waited_pid);

        // check reported exit status
        let pdata = p.data.get_mut();
        let pgt = pdata.page_table.as_mut().unwrap();
        let pa = pgt.walk_addr(0).expect("cannot walk") as *const u8;
        let reported_status = unsafe { (pa.offset(1) as *const i32).as_ref().unwrap() };
        assert_eq!(42i32, *reported_status);

        // restore
        pgt.unmap_pages(0, 1, true).expect("cannot unmap test code");
        pgt.uvm_init(&INITCODE)
            .expect("cannot map the initcode into the page");
    }
}
