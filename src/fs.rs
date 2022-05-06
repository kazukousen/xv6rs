use crate::{superblock::{read_super_block, SB}, log::LOG};

pub unsafe fn init(dev: u32) {
    read_super_block(dev);
    LOG.init(dev, &SB);
}

