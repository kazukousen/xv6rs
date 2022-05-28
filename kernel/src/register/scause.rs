use core::arch::asm;

#[inline]
unsafe fn read() -> usize {
    let ret: usize;
    asm!("csrr {}, scause", out(reg) ret);
    ret
}

const INTERRUPT: usize = 0x8000000000000000;
const INTERRUPT_SUPERVISOR_SOFTWARE: usize = INTERRUPT + 1;
const INTERRUPT_SUPERVISOR_EXTERNAL: usize = INTERRUPT + 9;
const EXCEPTION: usize = 0x0;
const EXCEPTION_ENVIRONMENT_CALL: usize = EXCEPTION + 8;

#[derive(Debug)]
pub enum ScauseType {
    IntSSoft,
    IntSExt,
    ExcEcall,
    Unknown(usize),
}

#[inline]
pub unsafe fn get_type() -> ScauseType {
    let scause = read();
    match scause {
        INTERRUPT_SUPERVISOR_SOFTWARE => ScauseType::IntSSoft,
        INTERRUPT_SUPERVISOR_EXTERNAL => ScauseType::IntSExt,
        EXCEPTION_ENVIRONMENT_CALL => ScauseType::ExcEcall,
        v => ScauseType::Unknown(v),
    }
}
