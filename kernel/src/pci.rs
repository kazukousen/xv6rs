use core::sync::atomic::{fence, Ordering};

use crate::{
    e1000::E1000,
    param::{E1000_REGS_ADDR, ECAM0},
    println,
};

pub fn init() {
    // look at each device on bus 0.
    for dev in 0..32 {
        let off = dev << 11;
        let base = unsafe { ((ECAM0 + off) as *mut usize as *mut [u32; 10]).as_mut() }.unwrap();
        let id = base[0];
        if id == 0x100e_8086 {
            // e1000
            println!("Initialzing e1000 ..."); // TODO: remove after debug

            // bit 0 : I/O access enable
            // bit 1 : memory access enable
            // bit 2 : enable mastering
            base[1] = 0x7;
            fence(Ordering::SeqCst);

            for i in 0..6 {
                let old = base[4 + i];
                // writing all 1's to the BAR causes it to be replaced with its size.
                base[4 + i] = 0xffff_ffff;
                fence(Ordering::SeqCst);
                base[4 + i] = old;
            }

            // tell the e1000 to reveal its registers at physical address E1000_BASE.
            base[4 + 0] = E1000_REGS_ADDR.try_into().unwrap();

            E1000.lock().init();
        }
    }
}
