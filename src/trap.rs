use core::mem;

use crate::{
    cpu::{CpuTable, CPU_TABLE},
    param::{TRAMPOLINE, TRAPFRAME, UART0_IRQ, VIRTIO0_IRQ},
    plic,
    register::{self, scause::ScauseType},
    spinlock::SpinLock,
    uart,
    virtio::DISK,
};

/// set up to take exceptions and traps while in the kernel.
pub unsafe fn init_hart() {
    extern "C" {
        fn kernelvec(); // in kernelvec.S
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

/// handle an interrupts, exceptions, or system call from user space.
/// trampoline jumps to here.
#[no_mangle]
pub unsafe extern "C" fn usertrap() {
    if register::sstatus::is_from_supervisor() {
        panic!("usertrap: not from user mode");
    }

    // send interrupts and exceptions to kerneltrap(), since we're now in the kernel.
    extern "C" {
        fn kernelvec(); // in kernelvec.S
    }
    register::stvec::write(kernelvec as usize);

    // save user program counter
    let p = CPU_TABLE.my_proc();
    let pdata = p.data.get_mut();
    pdata.set_epc(register::sepc::read());

    handle_trap(true);

    user_trap_ret();
}

unsafe fn handle_trap(is_user: bool) {
    let scause = register::scause::get_type();
    match scause {
        ScauseType::IntSSoft => {
            // software interrupt from a machine-mode timer interrupt,
            // forwarded by timervec in kernelvec.S.
            if CpuTable::cpu_id() == 0 {
                clock_intr();
            }

            register::sip::clear_ssip();

            CPU_TABLE.my_cpu_mut().yield_process();
        }
        ScauseType::IntSExt => {
            // this is a supervisor external interrupt, via PLIC.
            let irq = plic::claim();

            match irq as usize {
                UART0_IRQ => {
                    uart::intr();
                }
                VIRTIO0_IRQ => {
                    DISK.lock().intr();
                }
                0 => {}
                _ => panic!("irq type={}", irq),
            }

            if irq > 0 {
                plic::complete(irq);
            }
        }
        ScauseType::ExcEcall => {
            if !is_user {
                panic!("kerneltrap: handling syscall");
            }
            let p = CPU_TABLE.my_proc();
            p.syscall();
        }
        ScauseType::Unknown(v) => {
            panic!(
                "handle_trap: scause {:#x} stval {:#x}",
                v,
                register::stval::read()
            );
        }
    }
}

static TICKS: SpinLock<usize> = SpinLock::new(0);

fn clock_intr() {
    let mut guard = TICKS.lock();
    *guard += 1;
    drop(guard)
}

/// return to user space
pub unsafe fn user_trap_ret() {
    let p = CPU_TABLE.my_proc();
    let pdata = p.data.get_mut();

    // about to switch the destination of traps from kerneltrap() to usertrap(),
    // so turn off interrupt until back in user space where usertrap() is correct.
    register::sstatus::intr_off();

    extern "C" {
        fn uservec(); // in trampoline.S
        fn trampoline(); // in trampoline.S
    }

    // send syscalls, interrupts, and exceptions to user interrupt vector in trampoline.
    register::stvec::write(TRAMPOLINE + (uservec as usize - trampoline as usize));

    let satp = pdata.setup_user_ret();
    register::sstatus::intr_on_to_user();
    register::sepc::write(pdata.get_epc());

    // jump to trampoline.S at the top of memory, which
    // switches to the user page table, restores user registers,
    // and switches to user mode with sret.
    extern "C" {
        fn userret(); // in trampoline.S
    }
    let user_ret_virt = TRAMPOLINE + (userret as usize - trampoline as usize);
    let user_ret_virt: extern "C" fn(usize, usize) -> ! = mem::transmute(user_ret_virt);

    user_ret_virt(TRAPFRAME, satp);
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
