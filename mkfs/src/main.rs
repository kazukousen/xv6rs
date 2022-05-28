use std::env::args;
use std::io::Seek;
use std::ptr;
use std::{
    fs::{File, OpenOptions},
    io::{Read, SeekFrom, Write},
    mem,
};
use std::cmp::min;

const FSSIZE: usize = 1000; // size of file system in blocks
const DIRSIZ: usize = 14;
const BSIZE: usize = 1024; // size of disk block
const NDIRECT: usize = 12;
const NINDIRECT: usize = BSIZE / mem::size_of::<u32>();
const MAXFILE: usize = NDIRECT + NINDIRECT;
// number of inodes in a single block
const IPB: usize = BSIZE / mem::size_of::<DiskInode>();
const MAXOPBLOCKS: usize = 10; // max # of blocks any FS op writes
const LOGSIZE: usize = 3 * MAXOPBLOCKS;

const NINODES: usize = 200;
const NBITMAP: usize = FSSIZE / (BSIZE * 8) + 1;
const NINODEBLOCKS: usize = NINODES / IPB + 1;
const NLOG: usize = LOGSIZE;
const NMETA: usize = 2 + NLOG + NINODEBLOCKS + NBITMAP;
const NBLOCKS: usize = FSSIZE - NMETA;

const ROOTINO: u32 = 1;

#[repr(C)]
struct DirEnt {
    inum: u16,
    name: [u8; DIRSIZ],
}

impl DirEnt {
    fn empty() -> Self {
        Self {
            inum: 0,
            name: [0u8; DIRSIZ],
        }
    }
}

#[repr(u16)]
enum InodeType {
    Empty = 0,
    Directory = 1,
    File = 2,
    Device = 3,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct DiskInode {
    typ: u16,                  // file type
    major: u16,                // major device number (Device Type only)
    minor: u16,                // minor device number (Device Type only)
    nlink: u16,                // number of directory entries that refer to a file
    size: u32,                 // size of file (bytes)
    addrs: [u32; NDIRECT + 1], // data blocks addresses
}

impl DiskInode {
    fn new() -> Self {
        Self {
            typ: 0,
            major: 0,
            minor: 0,
            nlink: 0,
            size: 0,
            addrs: [0u32; NDIRECT + 1],
        }
    }
}

static mut SB: SuperBlock = SuperBlock::new();

#[repr(C)]
struct SuperBlock {
    magic: u32,
    size: u32,
    nblocks: u32,
    ninodes: u32,
    nlog: u32,
    logstart: u32,
    inodestart: u32,
    bmapstart: u32,
}

impl SuperBlock {
    const fn new() -> Self {
        Self {
            magic: 0x10203040,
            size: xint(FSSIZE as u32),
            nblocks: xint(NBLOCKS as u32),
            ninodes: xint(NINODES as u32),
            nlog: xint(NLOG as u32),
            logstart: xint(2),
            inodestart: xint(2 + NLOG as u32),
            bmapstart: xint(2 + NLOG as u32 + NINODEBLOCKS as u32),
        }
    }

    fn inode_block(&self, inum: u32) -> u32 {
        inum / u32::try_from(IPB).unwrap() + xint(self.inodestart)
    }
}

struct FSImage(File);

static mut FREE_INODE: u32 = 1;
static mut FREE_BLOCK: usize = NMETA;

impl FSImage {
    fn wsect(&mut self, sec: u32, buf: &[u8; BSIZE]) {
        self.0
            .seek(SeekFrom::Start((sec as usize * BSIZE) as u64))
            .expect("wsect: seek");
        self.0.write(buf).expect("wsect: write");
    }

    fn rsect(&mut self, sec: u32, buf: &mut [u8]) {
        self.0
            .seek(SeekFrom::Start((sec as usize * BSIZE) as u64))
            .expect("rsect: seek");

        self.0.read(buf).expect("rsect: read");
    }

    fn write_zeroes(&mut self) {
        let zeroes = [0u8; BSIZE];
        for i in 0..FSSIZE as u32 {
            self.wsect(i, &zeroes);
        }
    }

    fn ialloc(&mut self, typ: u16) -> u32 {
        let inum;
        unsafe {
            inum = FREE_INODE;
            FREE_INODE += 1;
        }

        let mut dinode = DiskInode::new();
        dinode.typ = xshort(typ);
        dinode.nlink = xshort(1);
        dinode.size = xint(0);
        self.winode(inum, dinode);
        inum
    }

    fn winode(&mut self, inum: u32, dinode: DiskInode) {
        let bn = unsafe { SB.inode_block(inum) };
        let mut buf = [0u8; BSIZE];
        self.rsect(bn, &mut buf);
        let dst =
            unsafe { (buf.as_mut_ptr() as *mut DiskInode).offset((inum as usize % IPB) as isize) };
        unsafe {
            ptr::write(dst, dinode);
        }
        self.wsect(bn, &buf);
    }

    fn rinode(&mut self, inum: u32, dinode: &mut DiskInode) {
        let bn = unsafe { SB.inode_block(inum) };
        let mut buf = [0u8; BSIZE];
        self.rsect(bn, &mut buf);
        let src =
            unsafe { (buf.as_ptr() as *const DiskInode).offset((inum as usize % IPB) as isize) };
        unsafe {
            ptr::write(dinode, *src.as_ref().unwrap());
        }
    }

    fn iappend(&mut self, inum: u32, mut src: *const u8, mut n: usize) {
        let mut dinode = DiskInode::new();
        self.rinode(inum, &mut dinode);
        let mut off = xint(dinode.size) as usize;
        while n > 0 {
            let fbn = off / BSIZE;
            assert!(fbn < MAXFILE);

            // lookup the block number
            let bn = if fbn < NDIRECT {
                if xint(dinode.addrs[fbn]) == 0 {
                    unsafe {
                        dinode.addrs[fbn] = xint(FREE_BLOCK as u32);
                        FREE_BLOCK += 1;
                    }
                }

                xint(dinode.addrs[fbn])
            } else {
                if xint(dinode.addrs[NDIRECT]) == 0 {
                    unsafe {
                        dinode.addrs[NDIRECT] = xint(FREE_BLOCK as u32);
                        FREE_BLOCK += 1;
                    }
                }
                let mut indirect = [0u32; NINDIRECT];
                unsafe { self.rsect(xint(dinode.addrs[NDIRECT]), (indirect.as_mut_ptr() as *mut [u8; NINDIRECT * mem::size_of::<u32>()]).as_mut().unwrap()) };
                if indirect[fbn - NDIRECT] == 0 {
                    unsafe {
                        indirect[fbn - NDIRECT] = xint(FREE_BLOCK as u32);
                        self.wsect(xint(dinode.addrs[NDIRECT]), (indirect.as_ptr() as *const [u8; NINDIRECT * mem::size_of::<u32>()]).as_ref().unwrap());
                        FREE_BLOCK += 1;
                    }
                }

                xint(indirect[fbn - NDIRECT])
            };

            let n1 = min(n, (fbn + 1) * BSIZE - off);
            let mut buf = [0u8; BSIZE];
            self.rsect(bn, &mut buf);
            unsafe{ ptr::copy(src, buf.as_mut_ptr().offset((off - fbn * BSIZE) as isize), n1) };
            self.wsect(bn, &buf);
            n -= n1;
            off += n1;
            src = unsafe { src.offset(n1 as isize) };
        }

        dinode.size = xint(off as u32);
        self.winode(inum, dinode);
    }

    fn balloc(&mut self, used: u16) {
        let used = used as usize;
        assert!(used < BSIZE * 8);
        let mut buf = [0u8; BSIZE];
        for i in 0..used {
            buf[i/8] = buf[i/8] | (0x1 << (i%8));
        }
        self.wsect(unsafe { xint(SB.bmapstart) }, &buf);
    }
}

const fn xshort(x: u16) -> u16 {
    x
    // let bytes = x.to_be_bytes();
    // (bytes[1] as u16) << 8 | bytes[0] as u16
}

const fn xint(x: u32) -> u32 {
    x
    // let bytes = x.to_be_bytes();
    // (bytes[3] as u32) << 24 | (bytes[2] as u32) << 16 | (bytes[1] as u32) << 8 | bytes[0] as u32
}

fn main() {
    assert!(BSIZE % mem::size_of::<DiskInode>() == 0);
    assert!(BSIZE % mem::size_of::<DirEnt>() == 0);

    let pathname = args().nth(1).expect("Usage: mkfs fs.img files...");

    // open or create fs.img
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(pathname)
        .expect("cannot open fs.img file");

    let mut fsimg = FSImage(f);
    fsimg.write_zeroes();

    // write superblock into the root inode
    let mut buf = [0u8; BSIZE];
    unsafe {
        ptr::copy_nonoverlapping(
            &SB as *const _ as *const u8,
            buf.as_mut_ptr(),
            mem::size_of::<SuperBlock>(),
        );
    }
    fsimg.wsect(1, &buf);

    let root_ino = fsimg.ialloc(InodeType::Directory as u16);
    assert_eq!(ROOTINO, root_ino);

    let mut de = DirEnt::empty();
    de.inum = xshort(root_ino as u16);
    // append directory entry "." into the root inode
    de.name[0] = b'.';
    fsimg.iappend(root_ino, &de as *const _ as *const u8, mem::size_of::<DirEnt>());
    // append directory entry ".." into the root inode
    de.name[1] = b'.';
    fsimg.iappend(root_ino, &de as *const _ as *const u8, mem::size_of::<DirEnt>());

    for user_prog in args().skip(2).into_iter() {
        println!("{}", user_prog);
        let mut f = File::open(&user_prog).unwrap();

        let mut user_prog = user_prog.as_str();
        if user_prog.len() > 5 && user_prog.as_bytes()[0..5] == [b'u', b's', b'e', b'r', b'/'] {
            user_prog = &user_prog[5..];
        }

        // Skip leading _ in name when writing to file system.
        // The binaries are named _rm, _cat, etc. to keep the
        // build operating system from trying to execute them
        // in place of system binaries like rm and cat.
        if user_prog.as_bytes()[0] == b'_' {
            user_prog = &user_prog[1..];
        }

        let inum = fsimg.ialloc(InodeType::File as u16);

        let mut de = DirEnt::empty();
        de.inum = xshort(inum as u16);
        for i in 0..user_prog.as_bytes().len() {
            de.name[i] = user_prog.as_bytes()[i];
        }
        fsimg.iappend(root_ino, &de as *const _ as *const u8, mem::size_of::<DirEnt>());

        let mut buf = [0u8; BSIZE];
        while f.read(&mut buf).unwrap() > 0 {
            fsimg.iappend(inum, buf.as_ptr(), BSIZE);
        }
        drop(f);
    }


    // fix size of root inode fir
    let mut dinode = DiskInode::new();
    fsimg.rinode(root_ino, &mut dinode);
    let mut off = xint(dinode.size) as usize;
    off = ((off / BSIZE) + 1) * BSIZE;
    dinode.size = xint(off as u32);
    fsimg.winode(root_ino, dinode);

    unsafe { fsimg.balloc(FREE_BLOCK.try_into().unwrap()) };

    drop(fsimg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_little_endians() {
        assert_eq!(256u16, xshort(1u16));
        assert_eq!(16777216u32, xint(1u32));
    }

    #[test]
    fn user_prog_shortname() {
        let mut user_prog = "user/sh.c";
        if user_prog.len() > 5 && user_prog.as_bytes()[0..5] == [b'u', b's', b'e', b'r', b'/'] {
            user_prog = &user_prog[5..];
        }
        assert_eq!("sh.c".as_bytes(), user_prog.as_bytes());
    }
}
