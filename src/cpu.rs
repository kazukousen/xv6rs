use core::ptr;

use crate::{
    param::NCPU,
    proc::{Context, Proc, ProcInner, ProcState},
    process::PROCESS_TABLE,
    register::{sstatus, tp},
    spinlock::SpinLockGuard,
};

use array_macro::array;

pub struct CpuTable {
    tables: [Cpu; NCPU],
}

pub static mut CPU_TABLE: CpuTable = CpuTable::new();

impl CpuTable {
    const fn new() -> Self {
        Self {
            tables: array![_ => Cpu::new(); NCPU],
        }
    }

    #[inline]
    pub fn cpu_id() -> usize {
        unsafe { tp::read() }
    }

    pub unsafe fn scheduler(&mut self) -> ! {
        extern "C" {
            fn swtch(old: *const Context, new: *mut Context); // in swtch.S
        }
        let cpu = self.my_cpu_mut();

        loop {
            // Avoid deadlock by ensuring that devices can interrupt.
            sstatus::intr_on();

            if let Some(p) = PROCESS_TABLE.find_runnable() {
                cpu.proc = p as *mut _;

                let mut guard = p.inner.lock();
                guard.state = ProcState::Running;

                // Save the scheduler context as soon as it is switched to the process's context.
                swtch(&cpu.scheduler as *const _, p.data.get_mut().get_context());

                // TODO: return called by sched()
                //
                // swtch called by `sched()` returns on the scheduler's stack as through
                // scheduler's switch had returned the scheduler continues its loop, finds a
                // process to run, switches to it, and the cycle repeats.

                cpu.proc = ptr::null_mut();
                drop(guard);
            }
        }
    }

    #[inline]
    pub fn my_cpu_mut(&mut self) -> &mut Cpu {
        let id = Self::cpu_id();
        &mut self.tables[id]
    }

    #[inline]
    fn my_cpu(&mut self) -> &Cpu {
        let id = Self::cpu_id();
        &self.tables[id]
    }

    pub fn my_proc(&mut self) -> &mut Proc {
        push_off();

        let p;

        let c = self.my_cpu();
        unsafe {
            p = &mut *c.proc;
        }

        pop_off();

        p
    }
}

pub struct Cpu {
    proc: *mut Proc,
    // save at the point in the past when `scheduler()` switched to the process's context.
    scheduler: Context,
    // Depth of push_off() nesting.
    // push_off/pop_off tracks to the nesting level of locks on the current CPU.
    noff: u8,
    // Were interruputs enabled before push_off()?
    intena: bool,
}

impl Cpu {
    const fn new() -> Self {
        Self {
            proc: ptr::null_mut(),
            scheduler: Context::new(),
            noff: 0,
            intena: false,
        }
    }

    /// Switch to cpu->scheduler, the per-CPU scheduler context that was saved at the point in the
    /// past when `scheduler()` called `swtch` to switch to the process that's giving up the CPU.
    /// Must hold only process's lock, must not hold another locks.
    /// Saves and restores intena because intena is a property of this kernel thread, not this CPU.
    /// Passing in and out a guard because we need to the lock during this function.
    pub fn sched<'a>(
        &mut self,
        guard: SpinLockGuard<'a, ProcInner>,
        ctx: *const Context,
    ) -> SpinLockGuard<'a, ProcInner> {
        if self.noff != 1 {
            panic!("sched: multi locks");
        }
        if guard.state == ProcState::Running {
            panic!("sched: proc is running");
        }
        if sstatus::intr_get() {
            panic!("sched: interruptable");
        }

        let intena = self.intena;

        extern "C" {
            fn swtch(old: *const Context, new: *mut Context); // in swtch.S
        }
        unsafe {
            swtch(ctx, &mut self.scheduler as *mut _);
        }

        self.intena = intena;

        guard
    }

    pub unsafe fn yield_process(&mut self) {
        if !self.proc.is_null() {
            let proc = self.proc.as_mut().unwrap();
            proc.yield_process();
        }
    }
}

/// `push_off()` are like `intr_off` to increment the nesting level of locks on the current CPU.
/// if it is called from the start of the outermost critical section, it saves the interrupt enable
/// state.
pub fn push_off() {
    let old = sstatus::intr_get();
    unsafe {
        sstatus::intr_off();
    }

    let cpu = unsafe { CPU_TABLE.my_cpu_mut() };
    if cpu.noff == 0 {
        cpu.intena = old;
    }
    cpu.noff += 1;
}

/// `pop_off()` are like `intr_on` to increment the nesting level of locks on the current CPU.
/// `noff` reaches zero, `pop_off()` restores the interrupt enable state that existed at the start
/// of the outermost critical section.
pub fn pop_off() {
    let cpu = unsafe { CPU_TABLE.my_cpu_mut() };
    if sstatus::intr_get() {
        panic!(
            "pop_off: already interruputable noff={} intena={}",
            cpu.noff, cpu.intena
        );
    }
    if cpu.noff < 1 {
        panic!("pop_off");
    }
    cpu.noff -= 1;

    if cpu.noff == 0 && cpu.intena {
        unsafe {
            sstatus::intr_on();
        }
    }
}
