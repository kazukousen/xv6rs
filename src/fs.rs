use crate::superblock::read_super_block;

pub unsafe fn init(dev: u32) {
    read_super_block(dev);
}
