use core::ptr;

use crate::bio::BCACHE;

pub static mut SB: SuperBlock = SuperBlock::new();
const FSMAGIC: u32 = 0x10203040;

pub unsafe fn read_super_block(dev: u32) {
    let buf = BCACHE.bread(dev, 1);

    ptr::copy_nonoverlapping(
        buf.data_ptr() as *const SuperBlock,
        &mut SB as *mut SuperBlock,
        1,
    );

    if SB.magic != FSMAGIC {
        panic!("invalid file system");
    }

    drop(buf);
}

#[repr(C)]
pub struct SuperBlock {
    magic: u32,
    pub size: u32,
    nblocks: u32,
    pub ninodes: u32,
    pub nlog: u32,
    pub logstart: u32,
    pub inodestart: u32,
    pub bmapstart: u32,
}

impl SuperBlock {
    const fn new() -> Self {
        Self {
            magic: 0,
            size: 0,
            nblocks: 0,
            ninodes: 0,
            nlog: 0,
            logstart: 0,
            inodestart: 0,
            bmapstart: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn read() {
        assert_eq!(FSMAGIC, unsafe { SB.magic });
        assert_eq!(200000, unsafe { SB.size });
        assert_eq!(32, unsafe { SB.inodestart });
        assert_eq!(36, unsafe { SB.bmapstart });
    }
}
