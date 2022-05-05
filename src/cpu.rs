use crate::{
    param::NCPU,
    register::{sstatus, tp},
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

    #[inline]
    fn my_cpu_mut(&mut self) -> &mut Cpu {
        let id = Self::cpu_id();
        &mut self.tables[id]
    }

    #[inline]
    fn my_cpu(&mut self) -> &Cpu {
        let id = Self::cpu_id();
        &self.tables[id]
    }
}

pub struct Cpu {
    // Depth of push_off() nesting.
    // push_off/pop_off tracks to the nesting level of locks on the current CPU.
    noff: u8,
    // Were interruputs enabled before push_off()?
    intena: bool,
}

impl Cpu {
    const fn new() -> Self {
        Self {
            noff: 0,
            intena: false,
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
