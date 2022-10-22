//! Term of `inode` can have two related meanings. On-disk data structure and In-memory.
//!
//! An inode describes a single unnamed file.
//! The inode disk structure holds metadata:
//! rhe file's type, its size, the number of links referring to it
//! and the list of blocks holding the file's content.
//!
//! The inodes are laid out sequentially on disk at SUPER_BLOCK.inodestart.
//!
//! The on-disk inode is structured by a `struct DiskInode`.
//!
//! The kernel keeps a table of in-use inodes in memory called `INODE_TABLE`
//! to provide a place for synchronizing access to inodes used by multiple processes.
//! The in-memory inodes include book-keeping information that is
//! not stored on disk: `refcnt` and `valid`.
//! The `refcnt` field counts the number of instances referring to the in-memory inode,
//! and kernel discards the inode from memory if the reference count drops to zero.
//! The `iget()` and `iput()` acquire and release an instance referring to an inode, modifying the reference count.
//!
//! the information in an node table entry is only correct when `valid` is some.
//! ilock() reads the inode from the disk and sets `valid`
//! while iput() clears `valid` if refcnt has fallen to zero.
//!
//! a typical sequence is:
//!
//!     let inode = INODE_TABLE.iget(dev, inum); // iget()
//!     let idata = inode.ilock(); // ilock()
//!     // examine and modify idata->xxx ...
//!     drop(idata); // iunlock()
//!     drop(inode); // iput()
//!
//! ilock() is separate from iget() so that system calls can
//! get a long-term reference to an inode (as for an open file)
//! and only lock it for shot periods (e.g. in read()).
//! The separation helps avoid deadlock and races during pathname lookup.
//! multiple process can hold an instance of an inode retuned by iget(),
//! but only one process can lock the inode at time.
//! iget() increments refcnt so that the inode stays in the table and pointers to it remain valid.

use core::{cmp::min, mem, ptr, str::from_utf8_unchecked};

use alloc::format;
use array_macro::array;

use crate::{
    bio::{BCACHE, BSIZE},
    bmap,
    cpu::CPU_TABLE,
    log::LOG,
    param::ROOTDEV,
    proc::{either_copy_in, either_copy_out},
    sleeplock::{SleepLock, SleepLockGuard},
    spinlock::SpinLock,
    superblock::{read_super_block, SB},
};

pub unsafe fn init(dev: u32) {
    read_super_block(dev);
    LOG.init(dev, &SB);
}

const NINODE: usize = 50;
// number of inodes in a single block
pub const IPB: usize = BSIZE / mem::size_of::<DiskInode>();
const DIRSIZ: usize = 30;
const ROOTINO: u32 = 1;

#[inline]
fn inode_offset(inum: u32) -> isize {
    (inum as usize % IPB) as isize
}

pub static INODE_TABLE: InodeTable = InodeTable::new();

pub struct InodeTable {
    meta: SpinLock<[InodeMeta; NINODE]>,
    data: [SleepLock<InodeData>; NINODE],
}

impl InodeTable {
    pub const fn new() -> Self {
        Self {
            meta: SpinLock::new(array![_ => InodeMeta::new(); NINODE], "inode_meta"),
            data: array![_ => SleepLock::new(InodeData::new(), "inode_data"); NINODE],
        }
    }

    /// Find the inode with number inum on device dev
    /// and return the in-memory copy.
    /// does not lock the inode and does not read it from disk.
    pub fn iget(&self, dev: u32, inum: u32) -> Inode {
        let mut guard = self.meta.lock();

        let mut empty: Option<usize> = None;
        for (i, ip) in guard.iter_mut().enumerate() {
            if ip.dev == dev && ip.inum == inum {
                ip.refcnt += 1;
                drop(guard);
                return Inode {
                    index: i,
                    dev,
                    inum,
                };
            }
            if empty.is_none() && ip.refcnt == 0 {
                empty = Some(i);
            }
        }

        let index = empty.unwrap();
        guard[index].dev = dev;
        guard[index].inum = inum;
        guard[index].refcnt = 1;
        drop(guard);

        let mut idata = self.data[index].lock();
        drop(idata.valid.take());
        drop(idata);

        Inode { index, dev, inum }
    }

    /// Drop a reference to an in-memory inode.
    /// if that was the last reference, the inode table entry can be recycled.
    /// if that was the last reference and the inode has no links to it,
    /// free the inode (and its content) on disk.
    /// all calls to iput() must be inside a transaction in case it has to free the inode.
    pub fn iput(&self, index: usize) {
        let mut guard = self.meta.lock();

        if guard[index].refcnt == 1 {
            // refcnt == 1 means no other process can have the inode locked,
            // so this sleep-lock won't block/deadlock.
            // that `1` is the reference owned by the thread calling `iput`.
            let mut idata = self.data[index].lock();
            if idata.valid.is_some() && idata.dinode.nlink == 0 {
                // inode has no links and no other references
                // truncate and free

                drop(guard);

                idata.itrunc();
                idata.dinode.typ = InodeType::Empty;
                idata.iupdate();
                drop(idata.valid.take());
                drop(idata);

                guard = self.meta.lock();
            } else {
                drop(idata);
            }
        }

        guard[index].refcnt -= 1;
        drop(guard);
    }

    /// Allocate an inode on device dev.
    /// Mark it as allocated by giving it type type.
    /// Returns an unlocked but allocated and referenced inode.
    ///
    /// it panics if the table have no inodes.
    fn ialloc(&self, dev: u32, typ: InodeType) -> Inode {
        for inum in 1..unsafe { SB.ninodes } {
            let mut buf = BCACHE.bread(dev, inode_block(inum));
            let dinode_ptr =
                unsafe { (buf.data_ptr_mut() as *mut DiskInode).offset(inode_offset(inum)) };
            let mut dinode = unsafe { dinode_ptr.as_mut().unwrap() };
            if dinode.typ == InodeType::Empty {
                // found a free inode
                unsafe { ptr::write_bytes(dinode_ptr, 0, 1) };
                dinode.typ = typ;
                // mark it allocated on the disk
                LOG.write(&mut buf);
                drop(buf);
                return self.iget(dev, inum);
            }
            drop(buf);
        }
        panic!("no free inodes")
    }

    /// if the name does not already exist, `create` now allocates a new inode with `ialloc`.
    /// if the new inode is a directory, `create` initializes it with `.` and `..` entries.
    /// finally, now that the data is initialized properly, `create` can link it into the parent
    /// directory.
    pub fn create(&self, path: &[u8], typ: InodeType, major: u16, minor: u16) -> Inode {
        // look up the parent dir inode
        let mut basename = [0u8; DIRSIZ];
        let parent_dir = self
            .nameiparent(&path, &mut basename)
            .expect("create: parent not found");
        let mut parent_dirdata = parent_dir.ilock();

        let inode = self.ialloc(parent_dir.dev, typ);
        let mut idata = inode.ilock();
        idata.dinode.major = major;
        idata.dinode.minor = minor;
        idata.dinode.nlink = 1;
        idata.iupdate();

        if typ == InodeType::Directory {
            // Create . and .. entries if the new inode is a directory.
            // No nlink++ for "." because avoid cyclic ref count.

            let mut name = [0u8; DIRSIZ];
            name[0] = b'.';
            idata
                .dirlink(&name, inode.inum)
                .expect("create: create with '.'");
            name[1] = b'.';
            idata
                .dirlink(&name, parent_dir.inum)
                .expect("create: create with '..'");
        }
        drop(idata);

        // link the new inode into the parent parent_dir.
        parent_dirdata.dirlink(&basename, inode.inum).expect(
            format!(
                "create: dirlink name={} inum={}",
                unsafe { from_utf8_unchecked(&basename) },
                inode.inum
            )
            .as_str(),
        );

        if typ == InodeType::Directory {
            parent_dirdata.dinode.nlink += 1; // for ".."
            parent_dirdata.iupdate();
        }

        drop(parent_dirdata);
        drop(parent_dir);

        inode
    }

    /// must be called inside a transaction (begin_op/end_op).
    pub fn unlink(&self, path: &[u8]) -> Result<(), &'static str> {
        let mut name = [0u8; DIRSIZ];
        let dir = self
            .nameiparent(&path, &mut name)
            .ok_or_else(|| "cannot find the parent")?;

        // Cannot unlink '.' or '..'
        let mut dot = [0u8; DIRSIZ];
        dot[0] = b'.';
        if &name == &dot {
            return Err("cannot unlink '.'");
        }
        dot[1] = b'.';
        if &name == &dot {
            return Err("cannot unlink '..'");
        }

        let mut dirdata = dir.ilock();
        let (inode, offset) = dirdata
            .dirlookup(&name)
            .ok_or_else(|| "cannot find the file in the parent")?;
        let mut idata = inode.ilock();

        if idata.dinode.nlink < 1 {
            panic!("unlink: nlink < 1");
        }
        if idata.dinode.typ == InodeType::Directory && !idata.is_dir_empty() {
            drop(idata);
            drop(inode);
            return Err("the directory not empty");
        }

        // update dirent empty
        let de = DirEnt::empty();
        dirdata
            .writei(
                false,
                &de as *const _ as *const u8,
                offset,
                mem::size_of::<DirEnt>(),
            )
            .expect("unlink: writei failed");

        if idata.dinode.typ == InodeType::Directory {
            dirdata.dinode.nlink -= 1;
            dirdata.iupdate();
        }
        drop(dirdata);
        drop(dir);

        idata.dinode.nlink -= 1;
        idata.iupdate();
        drop(idata);
        drop(inode);

        Ok(())
    }

    pub fn idup(&self, ip: &Inode) -> Inode {
        let mut guard = self.meta.lock();
        let i = ip.index;
        guard[i].refcnt += 1;
        Inode {
            dev: guard[i].dev,
            inum: guard[i].inum,
            index: i,
        }
    }

    /// Look up and return the inode for a given path name.
    ///
    /// if the path begins with a slash, evalution begins at the root, otherwise, the current
    /// directory.
    ///
    /// The procedure `namex` may take a long time to complete:
    ///     it could involve several disk operations to read inodes and directory blocks for the
    ///     directories traversed in the pathname (if they are not in the buffer cache).
    /// xv6 is carefully designed so that if invocation of `namex` by one kernel thread is blocked
    /// on a disk I/O, another kernel thread locking up a different pathname can proceed
    /// concurrency. `namex` locks each directory in the path separately so that lookups in
    /// different directories can proceed in parallel.
    ///
    /// The concurrency introduces some challenges. for example, while one kernel thread is locking
    /// up a pathname another kernel thread may be changing the directory tree unlinking a
    /// directory.
    /// A potential risk is that a lookup may be searching a directory that has been deleted by
    /// another kernel thread and its blocks have been re-used for another directory or file.
    /// `xv6` avoids such races. for example, when executing `dirlookup` in `namex`, the lookup
    /// thread holds the lock on the directory and `dirlookup` returns an inode that was obtained
    /// using `iget`. `iget` increases the reference count of the inode.
    /// only after receiving the inode from `dirlookup` does `namex` release the lock on the
    /// directory. now another thread may unlink the inode from the directory but xv6 will not
    /// delete the inode yet, because the reference count of the inode is still larger than zero.
    /// Another risk is deadlock. for example, `next` points to the same inode as `inode` when
    /// locking up ".". locking `next` before releasing the lock on `inode` would result in a
    /// deadlock. to avoid this deadlock, `namex` unlocks the directory before obtaining a lock on
    /// `next`. here again we see why the separation between `iget` and `ilock` is important.
    pub fn namex(&self, path: &[u8], name: &mut [u8; DIRSIZ], parent: bool) -> Option<Inode> {
        let mut inode = if path[0] == b'/' {
            self.iget(ROOTDEV, ROOTINO)
        } else {
            let cwd = unsafe { CPU_TABLE.my_proc().data.get_mut().cwd.as_ref().unwrap() };
            self.idup(cwd)
        };
        let mut path_pos = 0;
        loop {
            path_pos = self.skip_elem(path, path_pos, name);
            if path_pos == 0 {
                break;
            }

            // inode type is not guaranteed to have been loaded from disk until `ilock` runs.
            let mut idata = inode.ilock();

            if idata.dinode.typ != InodeType::Directory {
                drop(idata);
                return None;
            }

            if parent && path[path_pos] == 0 {
                // Stop one level early.
                drop(idata);
                return Some(inode);
            }

            match idata.dirlookup(name) {
                Some((next, _)) => {
                    drop(idata);
                    inode = next;
                }
                None => {
                    drop(idata);
                    return None;
                }
            }
        }

        Some(inode)
    }

    /// Lookup and return the inode for a pathname.
    /// must be called inside a transaction (begin_op/end_op) since it calls iput().
    pub fn namei(&self, path: &[u8]) -> Option<Inode> {
        let mut name: [u8; DIRSIZ] = [0; DIRSIZ];
        self.namex(path, &mut name, false)
    }

    pub fn nameiparent(&self, path: &[u8], name: &mut [u8; DIRSIZ]) -> Option<Inode> {
        self.namex(path, name, true)
    }

    /// Copy the next path element from path into name.
    /// Return the offset following the copied one.
    /// Examples:
    ///     skip_elem("a/bb/c", name) = 2, setting name = "a"
    ///     skip_elem("///a//bb", name) = 6, setting name = "a"
    ///     skip_elem("a", name) = 1, setting name = "a"
    ///     skip_elem("", name) = skip_elem("////", name) = 0
    fn skip_elem(&self, path: &[u8], mut cur: usize, name: &mut [u8; DIRSIZ]) -> usize {
        while path[cur] == b'/' {
            cur += 1;
        }
        if path[cur] == 0 {
            return 0;
        }

        let s = cur;

        while path[cur] != b'/' && path[cur] != 0 {
            cur += 1;
        }

        let mut len = cur - s;

        if len >= name.len() {
            len = name.len() - 1;
        }
        unsafe {
            ptr::copy_nonoverlapping(path.as_ptr().offset(s as isize), name.as_mut_ptr(), len);
        }
        name[len] = 0;

        while path[cur] == b'/' {
            cur += 1;
        }

        return cur;
    }
}

pub struct Inode {
    dev: u32,
    inum: u32,
    index: usize,
}

impl Inode {
    /// Lock the inode.
    /// Reads the inode from the disk if necessary.
    pub fn ilock(&self) -> SleepLockGuard<InodeData> {
        let mut guard = INODE_TABLE.data[self.index].lock();

        if guard.valid.is_some() {
            return guard;
        }

        // load on-disk structure inode.
        let buf = BCACHE.bread(self.dev, inode_block(self.inum));
        let dinode =
            unsafe { (buf.data_ptr() as *const DiskInode).offset(inode_offset(self.inum)) };
        guard.dinode = unsafe { dinode.as_ref().unwrap().clone() };
        drop(buf);

        if guard.dinode.typ == InodeType::Empty {
            panic!("ilock: no type");
        }

        guard.valid = Some((self.dev, self.inum));
        guard
    }
}

#[inline]
fn inode_block(inum: u32) -> u32 {
    inum / u32::try_from(IPB).unwrap() + unsafe { SB.inodestart }
}

impl Drop for Inode {
    fn drop(&mut self) {
        INODE_TABLE.iput(self.index);
    }
}

struct InodeMeta {
    dev: u32,
    inum: u32,
    refcnt: usize,
}

impl InodeMeta {
    const fn new() -> Self {
        Self {
            dev: 0,
            inum: 0,
            refcnt: 0,
        }
    }
}

/// it is always protected by sleep-lock.
pub struct InodeData {
    valid: Option<(u32, u32)>, // (dev, inum)
    dinode: DiskInode,
}

/// The on-disk inode structure `DiskInode`, contains a size and an array of block numbers.
/// The inode data is found in the blocks listed in the `DiskInode`'s `addrs` field array.
/// The first 12kB(NDIRECT x BSIZE) of a file can be loaded from the blocks listed in the inode,
/// while the next 256kB (NINDIRECT x BSIZE) can only be loaded after consulting the indirect
/// blocks.
/// (This is a good on-disk representation but a complex one for clients...)
impl InodeData {
    const fn new() -> Self {
        Self {
            valid: None,
            dinode: DiskInode::new(),
        }
    }

    #[inline]
    pub fn get_type(&self) -> InodeType {
        self.dinode.typ
    }

    #[inline]
    pub fn get_major(&self) -> u16 {
        self.dinode.major
    }

    /// Returns the disk block number of the offset'th data block in the inode.
    /// If there is no such block yet, bmap() allocates one.
    fn bmap(&mut self, mut offset: usize) -> u32 {
        let (dev, _) = self.valid.unwrap();

        if offset < NDIRECT {
            if self.dinode.addrs[offset] != 0 {
                return self.dinode.addrs[offset];
            }
            let data_bn = bmap::alloc(dev);
            self.dinode.addrs[offset] = data_bn;
            return data_bn;
        }

        // 0..11 12..267 268..(268 + 256 * 256)
        offset -= NDIRECT;

        if offset < NINDIRECT {
            // load the indirect block, allocating if necessary.
            let indirect_bn = if self.dinode.addrs[NDIRECT] != 0 {
                self.dinode.addrs[NDIRECT]
            } else {
                let bn = bmap::alloc(dev);
                self.dinode.addrs[NDIRECT] = bn;
                bn
            };
            // the block number of the data block.
            let mut buf = BCACHE.bread(dev, indirect_bn);
            let data_bn_ptr = unsafe { (buf.data_ptr_mut() as *mut u32).offset(offset as isize) };
            let data_bn = unsafe { ptr::read(data_bn_ptr) };
            if data_bn == 0 {
                let freed = bmap::alloc(dev);
                unsafe { ptr::write(data_bn_ptr, freed) };
                LOG.write(&mut buf);
            }
            drop(buf);
            return data_bn;
        }

        offset -= NINDIRECT;

        if offset < NDINDIRECT * NINDIRECT {
            // load the double indirect block, allocating if necessary.
            let d_indirect_bn = if self.dinode.addrs[NDIRECT + 1] != 0 {
                self.dinode.addrs[NDIRECT + 1]
            } else {
                let bn = bmap::alloc(dev);
                self.dinode.addrs[NDIRECT + 1] = bn;
                bn
            };
            // load the indirect block, allocating if necessary.
            let mut buf = BCACHE.bread(dev, d_indirect_bn);
            let indirect_bn_ptr =
                unsafe { (buf.data_ptr_mut() as *mut u32).offset((offset / NDINDIRECT) as isize) };
            let indirect_bn = unsafe { ptr::read(indirect_bn_ptr) };
            if indirect_bn == 0 {
                let freed = bmap::alloc(dev);
                unsafe { ptr::write(indirect_bn_ptr, freed) };
                LOG.write(&mut buf);
            }
            drop(buf);
            // the block number of the data block.
            let mut buf = BCACHE.bread(dev, indirect_bn);
            let data_bn_ptr =
                unsafe { (buf.data_ptr_mut() as *mut u32).offset((offset % NDINDIRECT) as isize) };
            let data_bn = unsafe { ptr::read(data_bn_ptr) };
            if data_bn == 0 {
                let freed = bmap::alloc(dev);
                unsafe { ptr::write(data_bn_ptr, freed) };
                LOG.write(&mut buf);
            }
            drop(buf);
            return data_bn;
        }

        panic!("bmap: out of range");
    }

    pub fn readi(
        &mut self,
        is_user: bool,
        mut dst: *mut u8,
        mut offset: usize,
        mut n: usize,
    ) -> Result<usize, ()> {
        let (dev, _) = self.valid.unwrap();

        let size = offset.checked_add(n).ok_or_else(|| ())?;

        let ret = if size > self.dinode.size as usize {
            self.dinode.size as usize - offset
        } else {
            n
        };
        n = ret;

        // copy the file to dst by separating it into multiparts.
        // [offset:BSIZE], [BSIZE:BSIZE*2], [BSIZE*N:n]
        while n > 0 {
            let read_n = min(BSIZE - offset % BSIZE, n);
            let buf = BCACHE.bread(dev, self.bmap(offset / BSIZE));
            let src_ptr =
                unsafe { (buf.data_ptr() as *const u8).offset((offset % BSIZE) as isize) };
            either_copy_out(is_user, dst, src_ptr, read_n);
            drop(buf);
            offset += read_n;
            n -= read_n;
            dst = unsafe { dst.offset(read_n as isize) };
        }

        Ok(ret)
    }

    /// Write data to inode.
    pub fn writei(
        &mut self,
        is_user: bool,
        mut src: *const u8,
        mut offset: usize,
        mut n: usize,
    ) -> Result<(), ()> {
        let (dev, _) = *self.valid.as_ref().unwrap();

        if offset > self.dinode.size as usize
            || offset.checked_add(n).ok_or_else(|| ())? > MAXFILE * BSIZE
        {
            return Err(());
        }

        while n > 0 {
            let write_n = min(n, BSIZE - offset % BSIZE);
            let mut buf = BCACHE.bread(dev, self.bmap(offset / BSIZE));
            let dst_ptr =
                unsafe { (buf.data_ptr_mut() as *mut u8).offset((offset % BSIZE) as isize) };
            either_copy_in(is_user, src, dst_ptr, write_n);
            LOG.write(&mut buf);
            drop(buf);
            offset += write_n;
            n -= write_n;
            src = unsafe { src.offset(write_n as isize) };
        }

        if offset > self.dinode.size as usize {
            self.dinode.size = offset.try_into().unwrap();
        }

        self.iupdate();

        Ok(())
    }

    pub fn stati(&self, dst: &mut FileStat) {
        let (dev, inum) = self.valid.unwrap();
        dst.dev = dev as i32;
        dst.inum = inum;
        dst.typ = self.dinode.typ;
        dst.nlink = self.dinode.nlink;
        dst.size = self.dinode.size as u64;
    }

    /// Look for a directory entry in a directory.
    fn dirlookup(&mut self, name: &[u8; DIRSIZ]) -> Option<(Inode, usize)> {
        let (dev, _) = self.valid.unwrap();
        if self.dinode.typ != InodeType::Directory {
            panic!("dirlookup not DIR");
        }

        let de_size = mem::size_of::<DirEnt>();
        let mut de = DirEnt::empty();
        let de_ptr = &mut de as *mut DirEnt as *mut u8;
        for off in (0..self.dinode.size as usize).step_by(de_size) {
            self.readi(false, de_ptr, off, de_size)
                .expect("dirlookup: read");

            if de.inum == 0 {
                continue;
            }

            // compare two names
            for i in 0..DIRSIZ {
                if de.name[i] != name[i] {
                    break;
                }
                if de.name[i] == 0 {
                    return Some((INODE_TABLE.iget(dev, de.inum as u32), off));
                }
            }
        }

        None
    }

    /// Truncate inode (discard contents).
    /// Caller must hold sleep-lock.
    pub fn itrunc(&mut self) {
        let (dev, _) = self.valid.unwrap();

        // direct blocks
        for i in 0..NDIRECT {
            if self.dinode.addrs[i] > 0 {
                bmap::free(dev, self.dinode.addrs[i]);
                self.dinode.addrs[i] = 0;
            }
        }

        // an indirect block
        if self.dinode.addrs[NDIRECT] > 0 {
            let buf = BCACHE.bread(dev, self.dinode.addrs[NDIRECT]);
            let bn_ptr = buf.data_ptr() as *const u32;
            for i in 0..(NINDIRECT as isize) {
                let bn = unsafe { ptr::read(bn_ptr.offset(i)) };
                if bn != 0 {
                    bmap::free(dev, bn);
                }
            }
            drop(buf);
            bmap::free(dev, self.dinode.addrs[NDIRECT]);
            self.dinode.addrs[NDIRECT] = 0;
        }

        self.dinode.size = 0;
        self.iupdate();
    }

    /// Copy a modified in-memory inode to disk.
    /// Must be called after every change to itself dinode field
    /// that lives on disk.
    /// Caller must hold sleep-lock.
    fn iupdate(&mut self) {
        let (dev, inum) = self.valid.unwrap();
        let mut buf = BCACHE.bread(dev, inode_block(inum));
        let dinode = unsafe { (buf.data_ptr() as *mut DiskInode).offset(inode_offset(inum)) };
        unsafe { ptr::write(dinode, self.dinode) };
        LOG.write(&mut buf);
    }

    /// Write a new directory entry (name, inum) into the directory this.
    fn dirlink(&mut self, name: &[u8; DIRSIZ], inum: u32) -> Result<(), &'static str> {
        if self.dinode.typ != InodeType::Directory {
            panic!("dirlink: not DIR");
        }

        // Check that name is not present.
        if let Some((inode, _)) = self.dirlookup(&name) {
            drop(inode);
            return Err("dirent already present");
        }

        // Look for the offset of an empty DirEnt.
        let mut de = DirEnt::empty();
        let offset = self.find_available_dirent_offset(&mut de)?;

        // update the dirent
        for i in 0..DIRSIZ {
            de.name[i] = name[i];
            if name[i] == 0 {
                break;
            }
        }
        de.inum = inum.try_into().unwrap();

        // write updated the dirent
        self.writei(
            false,
            &de as *const _ as *const u8,
            offset,
            mem::size_of::<DirEnt>(),
        )
        .or_else(|_| Err("failed to write dirent"))
    }

    /// returns the offset of the available entry.
    fn find_available_dirent_offset(&mut self, de: &mut DirEnt) -> Result<usize, &'static str> {
        let de_size = mem::size_of::<DirEnt>();

        for off in (0..self.dinode.size as usize).step_by(de_size) {
            self.readi(false, de as *mut _ as *mut u8, off, de_size)
                .or_else(|_| Err("failed to read entry in directory inode"))?;
            if de.inum == 0 {
                return Ok(off);
            }
        }

        Ok(self.dinode.size as usize)
    }

    /// Is the directory this empty except for '.' and '..' ?
    fn is_dir_empty(&mut self) -> bool {
        let mut de = DirEnt::empty();
        let de_size = mem::size_of::<DirEnt>();
        for off in (2 * de_size..self.dinode.size as usize).step_by(de_size) {
            self.readi(false, &mut de as *mut _ as *mut u8, off, de_size)
                .expect("is_dir_empty: failed to read entry in directory inode");
            if de.inum != 0 {
                return false;
            }
        }

        return true;
    }
}

const NDIRECT: usize = 11;
const NINDIRECT: usize = BSIZE / mem::size_of::<u32>();
const NDINDIRECT: usize = NINDIRECT;
const MAXFILE: usize = NDIRECT + NINDIRECT + NINDIRECT * NDINDIRECT;

/// On disk inode structure
#[repr(C)]
#[derive(Clone, Copy)]
struct DiskInode {
    typ: InodeType,            // file type
    major: u16,                // major device number (Device Type only)
    minor: u16,                // minor device number (Device Type only)
    nlink: u16,                // number of directory entries that refer to a file
    size: u32,                 // size of file (bytes)
    addrs: [u32; NDIRECT + 2], // data blocks addresses
}

impl DiskInode {
    const fn new() -> Self {
        Self {
            typ: InodeType::Empty,
            major: 0,
            minor: 0,
            nlink: 0,
            size: 0,
            addrs: [0; NDIRECT + 2],
        }
    }
}

#[repr(u16)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InodeType {
    Empty = 0,
    Directory = 1,
    File = 2,
    Device = 3,
}

#[repr(C)]
struct DirEnt {
    inum: u16,
    name: [u8; DIRSIZ],
}

impl DirEnt {
    const fn empty() -> Self {
        Self {
            inum: 0,
            name: [0; DIRSIZ],
        }
    }
}

#[repr(C)]
pub struct FileStat {
    dev: i32,
    inum: u32,
    typ: InodeType,
    nlink: u16,
    size: u64,
}

impl FileStat {
    pub fn uninit() -> Self {
        Self {
            dev: 0,
            inum: 0,
            typ: InodeType::Empty,
            nlink: 0,
            size: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use core::ops::Deref;

    use super::*;

    #[test_case]
    fn skip_elem_init() {
        let mut name = [0u8; DIRSIZ];
        let cur = INODE_TABLE.skip_elem(&[b'a', b'/', b'b', b'b', b'/', b'c'], 0, &mut name);
        assert_eq!(2, cur);
        let mut exp_name = [0u8; DIRSIZ];
        exp_name[0] = b'a';
        assert_eq!(&exp_name, &name);

        let mut name = [0u8; DIRSIZ];
        let cur = INODE_TABLE.skip_elem(&[b'/', b'i', b'n', b'i', b't', 0], 0, &mut name);
        assert_eq!(5, cur);

        let mut exp_name = [0u8; DIRSIZ];
        exp_name[0] = b'i';
        exp_name[1] = b'n';
        exp_name[2] = b'i';
        exp_name[3] = b't';
        assert_eq!(&exp_name, &name);
    }

    #[test_case]
    fn many_iget() {
        let i1 = INODE_TABLE.iget(ROOTDEV, ROOTINO);
        assert_eq!(0, i1.index);
        assert_eq!(ROOTDEV, i1.dev);
        assert_eq!(ROOTINO, i1.inum);

        let imeta = INODE_TABLE.meta.lock().deref() as *const [InodeMeta; NINODE];
        let imeta = unsafe { imeta.as_ref() }.unwrap();
        // recycle
        assert_eq!(2, imeta[0].refcnt); // (ROOTDEV, ROOTINO) is already used as superblock
        assert_eq!(ROOTDEV, imeta[0].dev);
        assert_eq!(ROOTINO, imeta[0].inum);

        let i2 = INODE_TABLE.iget(ROOTDEV, ROOTINO);
        // recycle
        assert_eq!(0, i2.index);
        assert_eq!(ROOTDEV, i2.dev);
        assert_eq!(ROOTINO, i2.inum);

        assert_eq!(3, imeta[0].refcnt);
        drop(i1);
        assert_eq!(2, imeta[0].refcnt);
        drop(i2);
        assert_eq!(1, imeta[0].refcnt);

        assert_eq!(ROOTDEV, imeta[0].dev);
        assert_eq!(ROOTINO, imeta[0].inum);
    }

    #[test_case]
    fn idup() {
        let i1 = INODE_TABLE.iget(ROOTDEV, ROOTINO);
        assert_eq!(0, i1.index);
        assert_eq!(ROOTDEV, i1.dev);
        assert_eq!(ROOTINO, i1.inum);

        let imeta = INODE_TABLE.meta.lock().deref() as *const [InodeMeta; NINODE];
        let imeta = unsafe { imeta.as_ref() }.unwrap();
        // recycle
        assert_eq!(2, imeta[0].refcnt); // (ROOTDEV, ROOTINO) is already used as superblock
        assert_eq!(ROOTDEV, imeta[0].dev);
        assert_eq!(ROOTINO, imeta[0].inum);

        let i2 = INODE_TABLE.idup(&i1);
        // recycle
        assert_eq!(0, i2.index);
        assert_eq!(ROOTDEV, i2.dev);
        assert_eq!(ROOTINO, i2.inum);

        assert_eq!(3, imeta[0].refcnt);
        drop(i1);
        assert_eq!(2, imeta[0].refcnt);
        drop(i2);
        assert_eq!(1, imeta[0].refcnt);

        assert_eq!(ROOTDEV, imeta[0].dev);
        assert_eq!(ROOTINO, imeta[0].inum);
    }

    #[test_case]
    fn lookup_root_init_by_dirlookup() {
        let inode = INODE_TABLE.iget(ROOTDEV, ROOTINO);
        let mut idata = inode.ilock();
        let mut f = [0u8; DIRSIZ];
        f[0] = b'i';
        f[1] = b'n';
        f[2] = b'i';
        f[3] = b't';
        let (init_inode, _) = idata.dirlookup(&f).expect("'init' not found in '/'");
        drop(init_inode);
        drop(idata);
    }

    #[test_case]
    fn lookup_not_exist_by_dirlookup() {
        let inode = INODE_TABLE.iget(ROOTDEV, ROOTINO);
        let mut idata = inode.ilock();
        let mut f = [0u8; DIRSIZ];
        f[0] = b'n';
        f[1] = b'o';
        f[2] = b't';
        f[3] = b'e';
        f[4] = b'x';
        f[5] = b'i';
        f[6] = b's';
        f[7] = b't';
        assert!(idata.dirlookup(&f).is_none());
    }

    #[test_case]
    fn lookup_root_init_by_namei() {
        let inode = INODE_TABLE
            .namei(&[b'/', b'i', b'n', b'i', b't', 0])
            .expect("'/init' not found");
    }

    #[test_case]
    fn lookup_by_nameiparent() {
        let mut name: [u8; DIRSIZ] = [0; DIRSIZ];
        let inode = INODE_TABLE
            .nameiparent(&[b'i', b'n', b'i', b't', 0], &mut name)
            .expect("the parent not found");
        let pdata = unsafe { CPU_TABLE.my_proc() }.data.get_mut();
        let cwd = pdata.cwd.as_ref().unwrap();
        assert_eq!(cwd.dev, inode.dev);
        assert_eq!(cwd.inum, inode.inum);
        drop(inode);

        let mut exp_name: [u8; DIRSIZ] = [0; DIRSIZ];
        exp_name[0] = b'i';
        exp_name[1] = b'n';
        exp_name[2] = b'i';
        exp_name[3] = b't';
        assert_eq!(&exp_name, &name);
    }

    #[test_case]
    fn lookup_by_dot() {
        let inode = INODE_TABLE.namei(&[b'.', 0]).expect("lookup");
        let pdata = unsafe { CPU_TABLE.my_proc() }.data.get_mut();
        let cwd = pdata.cwd.as_ref().unwrap();
        assert_eq!(cwd.dev, inode.dev);
        assert_eq!(cwd.inum, inode.inum);
        drop(inode);
    }

    #[test_case]
    fn test_create() {
        LOG.begin_op();
        let path = [
            b'c', b'r', b'e', b'a', b't', b'e', b'f', b'i', b'l', b'e', 0,
        ];
        let inode = INODE_TABLE.create(&path, InodeType::File, 0, 0);
        assert!(INODE_TABLE.unlink(&path).is_ok());
        drop(inode);
        LOG.end_op();
    }

    #[test_case]
    fn test_create_dir() {
        LOG.begin_op();
        // create a new dir
        let new_dir_path = [b'c', b'r', b'e', b'a', b't', b'e', b'd', b'i', b'r', 0];
        let inode = INODE_TABLE.create(&new_dir_path, InodeType::Directory, 0, 0);
        let new_inum = inode.inum;
        let idata = inode.ilock();
        assert_eq!(InodeType::Directory, idata.dinode.typ);
        assert_eq!(1u16, idata.dinode.nlink);
        drop(idata);
        drop(inode);
        LOG.end_op();

        // search the new dir
        let inode = INODE_TABLE.namei(&new_dir_path).expect("the dir not found");
        assert_eq!(new_inum, inode.inum);

        // dirlink '.', '..'
        let mut name: [u8; DIRSIZ] = [0; DIRSIZ];
        name[0] = b'.';
        let mut idata = inode.ilock();
        let (self_reference, _) = idata
            .dirlookup(&name)
            .expect(format!("cannot dirent `{}`", unsafe { from_utf8_unchecked(&name) }).as_str());
        assert_eq!(new_inum, self_reference.inum);
        drop(self_reference);
        name[1] = b'.';
        let (parent, _) = idata
            .dirlookup(&name)
            .expect(format!("cannot dirent `{}`", unsafe { from_utf8_unchecked(&name) }).as_str());
        assert_eq!(1u32, parent.inum);
        drop(parent);
        drop(idata);

        // change directory
        let pdata = unsafe { CPU_TABLE.my_proc() }.data.get_mut();
        let root_dir = pdata.cwd.replace(inode);
        drop(root_dir);

        // lookup the parent
        // the parent must be the root
        let mut name: [u8; DIRSIZ] = [0; DIRSIZ];
        let parent = INODE_TABLE
            .nameiparent(
                &[
                    b'.', b'.', b'/', b'c', b'r', b'e', b'a', b't', b'e', b'd', b'i', b'r', 0,
                ],
                &mut name,
            )
            .expect("lookup");
        assert_eq!(1u32, parent.inum);

        LOG.begin_op();
        // tidy up
        // change the current directory
        let pdata = unsafe { CPU_TABLE.my_proc() }.data.get_mut();
        let root = INODE_TABLE
            .namei(&[b'/', 0])
            .expect("cannot find root inode by b'/'");
        let inode = pdata.cwd.replace(root).unwrap();
        assert_eq!(new_inum, inode.inum);
        drop(inode);
        LOG.end_op();
    }
}
