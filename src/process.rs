use array_macro::array;

use crate::{
    kvm::kvm_map,
    page_table::{Page, PteFlag, QuadPage},
    param::{KSTACK_SIZE, PAGESIZE, TRAMPOLINE},
    proc::{Proc, ProcState},
};

pub const NPROC: usize = 64;

pub struct ProcessTable {
    tables: [Proc; NPROC],
}

pub static mut PROCESS_TABLE: ProcessTable = ProcessTable::new();

impl ProcessTable {
    const fn new() -> Self {
        Self {
            tables: array![_ => Proc::new(); NPROC],
        }
    }

    /// Initialize the process table at boot time.
    /// Allocate pages for each process's kernel stack.
    pub fn init(&mut self) {
        for (i, p) in self.tables.iter_mut().enumerate() {
            // map kernel stacks beneath the trampoline,
            // each surrounded by invalid guard pages.
            let va = Self::calc_kstack_va(i);
            let pa = unsafe { QuadPage::alloc_into_raw() }
                .expect("process_table: insufficient memory for process's kernel stack");
            unsafe { kvm_map(va, pa as usize, KSTACK_SIZE, PteFlag::READ | PteFlag::WRITE) };
            p.data.get_mut().set_kstack(va);
        }
    }

    #[inline]
    fn calc_kstack_va(i: usize) -> usize {
        TRAMPOLINE - ((i + 1) * (KSTACK_SIZE + PAGESIZE))
    }

    fn alloc_proc(&mut self) -> Option<&mut Proc> {
        for p in self.tables.iter_mut() {
            let mut guard = p.inner.lock();
            if guard.state == ProcState::Unused {
                // found an used process
                let pdata = p.data.get_mut();
                pdata.init_trapframe().ok()?;
                pdata.init_context();
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

        // TODO: p.cwd = fs::namei("/");
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
        }
        None
    }
}
