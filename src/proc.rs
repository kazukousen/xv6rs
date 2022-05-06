use core::{cell::UnsafeCell, mem, ptr};

use alloc::boxed::Box;

use crate::{
    cpu::{CpuTable, CPU_TABLE},
    fs,
    page_table::{Page, PageTable, SinglePage},
    param::{KSTACK_SIZE, PAGESIZE, ROOTDEV},
    println,
    register::satp,
    spinlock::{SpinLock, SpinLockGuard},
    trap::{user_trap_ret, usertrap},
};

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
}

impl ProcInner {
    const fn new() -> Self {
        Self {
            state: ProcState::Unused,
            chan: 0,
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
}

pub struct ProcData {
    kstack: usize,
    sz: usize,
    page_table: Option<Box<PageTable>>,
    trapframe: *mut TrapFrame,
    context: Context,
}

impl ProcData {
    const fn new() -> Self {
        Self {
            kstack: 0,
            sz: 0,
            page_table: None,
            trapframe: ptr::null_mut(),
            context: Context::new(),
        }
    }

    pub fn set_kstack(&mut self, v: usize) {
        self.kstack = v;
    }

    pub fn init_trapframe(&mut self) -> Result<(), ()> {
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

    pub fn user_init(&mut self) -> Result<(), &'static str> {
        // allocate one user page and copy init's instructions
        // and data into it.
        self.page_table.as_mut().unwrap().uvm_init(&INITCODE)?;
        self.sz = PAGESIZE;

        // prepare for the very first "return" from kernel to user.
        let trapframe = unsafe { self.trapframe.as_mut().unwrap() };
        trapframe.epc = 0; // user program counter
        trapframe.sp = PAGESIZE; // user stack poiner
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

    pub fn syscall(&mut self) {
        let trapframe = unsafe { self.trapframe.as_mut() }.unwrap();

        // sepc points to the ecall instruction,
        // but we want to return to the next instruction.
        trapframe.epc += 4;

        let num = trapframe.a7;
        let ret = match num {
            7 => self.sys_exec(),
            _ => {
                panic!("unknown syscall: {}", num);
            }
        };
        trapframe.a0 = match ret {
            Ok(ret) => ret,
            Err(msg) => {
                println!("syscall error: {}", msg);
                -1isize as usize
            }
        };

        panic!("syscall: no={}", num);
    }

    #[inline]
    fn arg_str(&self, n: usize, dst: &mut [u8]) -> Result<usize, &'static str> {
        let addr = self.arg_raw(n)?;
        self.fetch_str(addr, dst)
    }

    #[inline]
    fn fetch_str(&self, addr: usize, dst: &mut [u8]) -> Result<usize, &'static str> {
        self.page_table.as_ref().unwrap().copy_in_str(dst, addr)
    }

    #[inline]
    fn arg_raw(&self, n: usize) -> Result<usize, &'static str> {
        let tf = unsafe { self.trapframe.as_ref().unwrap() };
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
    fn fetch_addr(&self, addr: usize) -> Result<usize, &'static str> {
        if addr >= self.sz || addr + mem::size_of::<usize>() > self.sz {
            return Err("fetch_addr size");
        }
        let mut dst: usize = 0;
        self.page_table.as_ref().unwrap().copy_in(
            &mut dst as *mut usize as *mut u8,
            addr,
            mem::size_of::<usize>(),
        )?;
        Ok(dst)
    }
}

pub struct Proc {
    pub inner: SpinLock<ProcInner>,
    pub data: UnsafeCell<ProcData>,
}

impl Proc {
    pub const fn new() -> Self {
        Self {
            inner: SpinLock::new(ProcInner::new()),
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
}

static mut FIRST: bool = true;

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
