use core::{ops::DerefMut, ptr};

use crate::{
    bio::{BufGuard, BCACHE},
    cpu::CPU_TABLE,
    param::MAXOPBLOCKS,
    process::PROCESS_TABLE,
    spinlock::SpinLock,
    superblock::SuperBlock,
};

pub const LOGSIZE: usize = MAXOPBLOCKS * 3; // max data blocks in on-disk log

#[repr(C)]
struct LogHeader {
    n: u32,
    blocknos: [u32; LOGSIZE],
}

impl LogHeader {
    const fn new() -> Self {
        Self {
            n: 0,
            blocknos: [0; LOGSIZE],
        }
    }
}

pub struct Log {
    start: u32,
    size: u32,
    outstanding: usize, // how many FS sys calls are executing.
    committing: bool,   // in commit(), please wait.
    dev: u32,
    header: LogHeader,
}

pub static LOG: SpinLock<Log> = SpinLock::new(Log::new(), "log");

impl SpinLock<Log> {
    pub unsafe fn init(&self, dev: u32, sb: &SuperBlock) {
        // must be called without holding locks, since not allowed to sleep with locks.
        let log = self.lock().deref_mut() as *mut Log;
        log.as_mut().unwrap().init(dev, sb);
    }

    /// called at the start of each FS system call.
    /// waits if it is commiting or has free log space.
    /// otherwise, increases outstanding.
    pub fn begin_op(&self) {
        let mut guard = self.lock();
        loop {
            if guard.committing {
                // wait during commiting
                unsafe {
                    guard = CPU_TABLE
                        .my_proc()
                        .sleep(&guard as *const _ as usize, guard);
                }
                continue;
            }

            if guard.header.n as usize + (guard.outstanding + 1) * MAXOPBLOCKS > LOGSIZE {
                // wait free log space
                unsafe {
                    guard = CPU_TABLE
                        .my_proc()
                        .sleep(&guard as *const _ as usize, guard);
                }
                continue;
            }

            // sys call reserved log space
            guard.outstanding += 1;
            drop(guard);
            break;
        }
    }

    /// typically replaces bwrite().
    /// Caller has modified buf->data and is done with the buffer.
    /// Record the block number and pin in the cache by increasing refcnt.
    /// commit()/write_log() will do the disk write.
    pub fn write(&self, buf: &mut BufGuard) {
        let mut guard = self.lock();

        if (guard.header.n as usize) >= LOGSIZE || guard.header.n >= guard.size - 1 {
            panic!("log_write: too big a transaction");
        }
        if guard.outstanding < 1 {
            panic!("log_write: out of trans");
        }

        let mut i = 0usize;
        while i < guard.header.n as usize {
            if guard.header.blocknos[i] == buf.blockno {
                // log absorption to optimize for many writes
                break;
            }
            i += 1;
        }

        guard.header.blocknos[i] = buf.blockno;

        if i == guard.header.n as usize {
            // Add new block to log?
            unsafe {
                buf.bpin();
            }
            guard.header.n += 1;
        }
        drop(guard);
    }

    /// called at the end of each FS system call.
    /// commits if this was the last outstanding operation.
    pub fn end_op(&self) {
        let mut guard = self.lock();
        guard.outstanding -= 1;
        if guard.committing {
            panic!("log end_op: committing");
        }
        let do_commit = guard.outstanding == 0;
        if do_commit {
            guard.committing = true;
        } else {
            // begin_op may be waiting for log space
            unsafe {
                PROCESS_TABLE.wakeup(&guard as *const _ as usize);
            }
        }
        drop(guard); // release the spin-lock once to call commit

        if do_commit {
            // call commit without locks, since not allowed to sleep with locks.
            let log = self.lock().deref_mut() as *mut Log;
            unsafe {
                log.as_mut().unwrap().commit();
            }

            let mut guard = self.lock();
            guard.committing = false;
            unsafe {
                PROCESS_TABLE.wakeup(&guard as *const _ as usize);
            }
            drop(guard);
        }
    }
}

impl Log {
    const fn new() -> Self {
        Self {
            start: 0,
            size: 0,
            outstanding: 0,
            committing: false,
            dev: 0,
            header: LogHeader::new(),
        }
    }

    fn init(&mut self, dev: u32, sb: &SuperBlock) {
        self.start = sb.logstart;
        self.size = sb.nlog;
        self.dev = dev;
        self.recover_from_log();
    }

    fn recover_from_log(&mut self) {
        self.read_head();
        self.install_trans(true);
        self.header.n = 0;
        self.write_head();
    }

    fn read_head(&mut self) {
        let buf = BCACHE.bread(self.dev, self.start);

        unsafe {
            ptr::copy_nonoverlapping(buf.data_ptr() as *const LogHeader, &mut self.header, 1);
        }
        drop(buf);
    }

    /// Copy committed blocks from log to their home location
    fn install_trans(&mut self, recovering: bool) {
        for tail in 0..self.header.n {
            let log_buf = BCACHE.bread(self.dev, self.start + tail + 1); // read log block
            let mut disk_buf = BCACHE.bread(self.dev, self.header.blocknos[tail as usize]); // read dst block
            unsafe {
                ptr::copy_nonoverlapping(log_buf.data_ptr(), disk_buf.data_ptr_mut(), 1);
            }
            disk_buf.bwrite();
            if !recovering {
                unsafe {
                    disk_buf.bunpin();
                }
            }
            drop(log_buf);
            drop(disk_buf);
        }
    }

    fn write_head(&mut self) {
        let mut buf = BCACHE.bread(self.dev, self.start);

        unsafe {
            ptr::copy_nonoverlapping(&self.header, buf.data_ptr_mut() as *mut LogHeader, 1);
        }
        buf.bwrite();
        drop(buf);
    }

    fn commit(&mut self) {
        if self.header.n > 0 {
            self.write_log(); // write modified blocks from cache to log
            self.write_head(); // write header to disk -- real commit
            self.install_trans(false); // now install writes to home locations
            self.header.n = 0;
            self.write_head(); // erase the transaction from the log
        }
    }

    fn write_log(&mut self) {
        for tail in 0..self.header.n {
            let from = BCACHE.bread(self.dev, self.header.blocknos[tail as usize]);
            let mut to = BCACHE.bread(self.dev, self.start + tail + 1);
            unsafe {
                ptr::copy_nonoverlapping(from.data_ptr(), to.data_ptr_mut(), 1);
            }
            to.bwrite();
            drop(from);
            drop(to);
        }
    }
}
