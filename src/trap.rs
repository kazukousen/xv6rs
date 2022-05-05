use crate::{
    cpu::CpuTable,
    register::{self, scause::ScauseType},
    spinlock::SpinLock,
};

/// set up to take exceptions and traps while in the kernel.
pub unsafe fn init_hart() {
    extern "C" {
        fn kernelvec();
    }
    register::stvec::write(kernelvec as usize);
}

/// kernelvec jumps here
#[no_mangle]
pub unsafe fn kerneltrap() {
    let sepc = register::sepc::read();
    let sstatus = register::sstatus::read();

    if !register::sstatus::is_from_supervisor() {
        panic!("kerneltrap: not from supervisor mode");
    }

    if register::sstatus::intr_get() {
        panic!("kerneltrap: interrupts enabled");
    }

    handle_trap(false);

    // the yield() may have caused some traps to occur,
    // so restore trap registers for use by kernelvec.S's sepc instruction.
    register::sepc::write(sepc);
    register::sstatus::write(sstatus);
}

unsafe fn handle_trap(_is_user: bool) {
    let scause = register::scause::get_type();
    match scause {
        ScauseType::IntSSoft => {
            // software interrupt from a machine-mode timer interrupt,
            // forwarded by timervec in kernelvec.S.
            if CpuTable::cpu_id() == 0 {
                clock_intr();
            }

            register::sip::clear_ssip();

            // TODO: yield running process
        }
        ScauseType::Unknown(v) => {
            panic!("handle_trap: scause {:#x}", v);
        }
    }
}

static TICKS: SpinLock<usize> = SpinLock::new(0);

fn clock_intr() {
    let mut guard = TICKS.lock();
    *guard += 1;
    drop(guard)
}

#[cfg(test)]
mod tests {
    use super::*;

    // a timer interrupt should occur before the entry point for `cargo test` is reached.
    #[test_case]
    fn increment_ticks() {
        let ticks = TICKS.lock();
        assert!(*ticks > 0);
        drop(ticks);
    }
}
