use core::{mem, ptr};

use array_macro::array;

use crate::{
    cpu::CPU_TABLE,
    kvm::kvm_map,
    log::LOG,
    page_table::{Page, PteFlag, QuadPage},
    param::{KSTACK_SIZE, PAGESIZE, TRAMPOLINE},
    proc::{Proc, ProcState},
    spinlock::SpinLock,
};

pub const NPROC: usize = 64;

pub struct ProcessTable {
    tables: [Proc; NPROC],
    pid: SpinLock<usize>,
    pub parents: SpinLock<[Option<usize>; NPROC]>,
}

pub static mut PROCESS_TABLE: ProcessTable = ProcessTable::new();

impl ProcessTable {
    const fn new() -> Self {
        Self {
            tables: array![i => Proc::new(i); NPROC],
            pid: SpinLock::new(0, "pid"),
            parents: SpinLock::new([None; NPROC], "parents"),
        }
    }

    /// Initialize the process table at boot time.
    /// Allocate pages for each process's kernel stack.
    pub fn init(&mut self) {
        for (i, p) in self.tables.iter_mut().enumerate() {
            // map the kernel stacks beneath the trampoline, each surrounded by invalid guard pages.
            let va = TRAMPOLINE - ((i + 1) * (KSTACK_SIZE + PAGESIZE));
            let pa = unsafe { QuadPage::alloc_into_raw() }
                .expect("process_table: insufficient memory for process's kernel stack");
            unsafe { kvm_map(va, pa as usize, KSTACK_SIZE, PteFlag::READ | PteFlag::WRITE) };
            p.data.get_mut().set_kstack(va);
        }
    }

    #[inline]
    fn alloc_pid(&mut self) -> usize {
        let ret: usize;
        let mut pid = self.pid.lock();
        ret = *pid;
        *pid += 1;
        drop(pid);
        ret
    }

    pub fn alloc_proc(&mut self) -> Option<&mut Proc> {
        let pid = self.alloc_pid();
        for p in self.tables.iter_mut() {
            let mut guard = p.inner.lock();
            if guard.state == ProcState::Unused {
                // found an used process
                let pdata = p.data.get_mut();
                pdata.init_page_table().ok()?;
                pdata.init_context();

                guard.pid = pid;
                guard.state = ProcState::Allocated;

                drop(guard);
                return Some(p);
            }
            drop(guard);
        }
        None
    }

    pub fn user_init(&mut self) {
        let p = self.alloc_proc().expect("user_init: no proc frees");

        p.data
            .get_mut()
            .user_init()
            .expect("user_init: failed process's initilization");

        p.inner.lock().state = ProcState::Runnable;
    }

    pub fn find_runnable(&mut self) -> Option<&mut Proc> {
        for p in self.tables.iter_mut() {
            let mut guard = p.inner.lock();
            if guard.state == ProcState::Runnable {
                // found a runnable process
                guard.state = ProcState::Allocated;
                drop(guard);
                return Some(p);
            }
            drop(guard);
        }
        None
    }

    pub fn wakeup(&self, chan: usize) {
        for p in self.tables.iter() {
            unsafe {
                if ptr::eq(p, CPU_TABLE.my_proc()) {
                    continue;
                }
            }
            let mut guard = p.inner.lock();
            if guard.state == ProcState::Sleeping && guard.chan == chan {
                guard.state = ProcState::Runnable;
            }
            drop(guard);
        }
    }

    /// waits for a child of the given process `p` to exit. copies exit status into `addr`.
    pub fn wait(&mut self, p: &mut Proc, addr: usize) -> Result<usize, &'static str> {
        let mut parents = self.parents.lock();

        loop {
            let mut have_kids = false;
            for i in 0..NPROC {
                // is this child of p?
                if parents[i].is_none() || parents[i].unwrap() != p.index {
                    continue;
                }
                // make sure the child isn't still in exit() or swtch()
                let child = &mut self.tables[i];
                let mut cguard = child.inner.lock();

                have_kids = true;

                if cguard.state != ProcState::Zombie {
                    // the child is still working
                    drop(cguard);
                    continue;
                }

                if addr != 0 {
                    // copy exit status into `addr`
                    if let Err(msg) = p.data.get_mut().copy_out(
                        addr,
                        &cguard.exit_status as *const _ as *const u8,
                        mem::size_of::<i32>(),
                    ) {
                        drop(cguard);
                        drop(parents);
                        return Err(msg);
                    }
                }

                // take pid for ret
                let child_pid = cguard.pid;

                // tidy up
                let cdata = child.data.get_mut();
                Proc::free(cdata, cguard);
                parents[child.index].take();

                return Ok(child_pid);
            }

            let killed: bool;
            let pguard = p.inner.lock();
            killed = pguard.killed;
            drop(pguard);

            // No point waiting if we don't have any children
            if !have_kids || killed {
                drop(parents);
                return Err("children not found");
            }

            // wait for a child to exit, use the parent's pointer as chan
            parents = p.sleep(p as *const Proc as usize, parents);
        }
    }

    /// terminates the given process `p`. status reported to wait(). no returns.
    pub fn exit(&self, p: &mut Proc, status: i32) {
        if ptr::eq(&self.tables[0] as *const _, p) {
            panic!("init exiting");
        }

        // close all open files
        let pdata = p.data.get_mut();
        for i in 0..pdata.o_files.len() {
            pdata.o_files[i].take();
        }

        LOG.begin_op();
        drop(pdata.cwd.take());
        LOG.end_op();

        let mut parents = self.parents.lock();

        // reparent.
        // pass the process's abandoned children to the init proc.
        for i in 1..NPROC {
            if parents[i].is_some() && parents[i].unwrap() == p.index {
                parents[i] = Some(0); // init proc
                self.wakeup(&self.tables[0] as *const Proc as usize);
            }
        }

        // processes other than the init proc must have own parent because their are always created by
        // fork().
        let parent = *parents[p.index].as_ref().unwrap();
        // its parent might be sleeping in wait().
        self.wakeup(&self.tables[parent] as *const Proc as usize);

        let mut pguard = p.inner.lock();
        pguard.exit_status = status;
        pguard.state = ProcState::Zombie;

        drop(parents);

        // jump into the scheduler, never to retun.
        unsafe { CPU_TABLE.my_cpu_mut() }.sched(pguard, pdata.get_context());
        unreachable!();
    }
}
