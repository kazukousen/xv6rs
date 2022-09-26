use core::cell::UnsafeCell;
use core::mem;

use alloc::{boxed::Box, vec, vec::Vec};
use array_macro::array;

use crate::{
    cpu::CPU_TABLE, mbuf::MBuf, proc::either_copy_out, process::PROCESS_TABLE, spinlock::SpinLock,
};

use super::{
    ip::{self, Protocol},
    toggle_endian16, toggle_endian32, SockAddr, USABLE_PORT_MAX, USABLE_PORT_MIN,
};

#[repr(C, packed)]
struct Header {
    src_port: u16,
    dst_port: u16,
    len: u16,
    checksum: u16,
}

pub const HEADER_SIZE: usize = mem::size_of::<Header>();

struct ControlBlock {
    idx: usize,
    meta: UnsafeCell<ControlBlockMeta>,
    data: SpinLock<ControlBlockData>,
}

impl ControlBlock {
    const fn new(idx: usize) -> Self {
        Self {
            idx,
            meta: UnsafeCell::new(ControlBlockMeta::new()),
            data: SpinLock::new(ControlBlockData::empty(), "udp_cb_data"),
        }
    }
}

struct ControlBlockMeta {
    used: bool,
    port: u16,
    peer_addr: u32,
    peer_port: u16,
}

impl ControlBlockMeta {
    const fn new() -> Self {
        Self {
            used: false,
            port: 0,
            peer_addr: 0,
            peer_port: 0,
        }
    }
}

struct ControlBlockData {
    mbuf_queue: Vec<Box<MBuf>>,
}

impl ControlBlockData {
    const fn empty() -> Self {
        Self { mbuf_queue: vec![] }
    }
}

const CB_TABLE_SIZE: usize = 16;

static mut TABLE: Table = Table::new();

pub struct Table {
    table: [ControlBlock; CB_TABLE_SIZE],
    locker: SpinLock<()>,
}

impl Table {
    const fn new() -> Self {
        Self {
            table: array![i => ControlBlock::new(i); CB_TABLE_SIZE],
            locker: SpinLock::new((), "udp_table"),
        }
    }

    fn find(&self, port: u16) -> Option<&ControlBlock> {
        for cb in self.table.iter() {
            let meta = unsafe { &*cb.meta.get() };
            if meta.used && meta.port == port {
                return Some(cb);
            }
        }
        None
    }
}

/// reserves ControlBlock
pub fn open() -> Result<usize, &'static str> {
    let _guard = unsafe { TABLE.locker.lock() };
    for cb in unsafe { &mut TABLE.table }.iter_mut() {
        let meta = cb.meta.get_mut();
        if meta.used {
            continue;
        }
        meta.used = true;
        return Ok(cb.idx);
    }

    Err("udp_open: ControlBlock unavailable")
}

pub fn close(idx: usize) -> Result<(), &'static str> {
    let _guard = unsafe { TABLE.locker.lock() };
    let cb = unsafe { &mut TABLE.table[idx] };
    if !cb.meta.get_mut().used {
        return Err("udp_close: unopend");
    }

    cb.meta = UnsafeCell::new(ControlBlockMeta::new());
    cb.data = SpinLock::new(ControlBlockData::empty(), "udp_cb_data");

    Ok(())
}

pub fn bind(idx: usize, addr: &SockAddr) -> Result<(), &'static str> {
    let cb = unsafe { &TABLE.table[idx] };
    let meta = unsafe { &mut *cb.meta.get() };
    if !meta.used {
        return Err("udp_bind: not opened");
    }

    meta.port = addr.port;

    Ok(())
}

/// updates the connection's metadata
pub fn connect(idx: usize, dst_addr: &SockAddr) -> Result<(), &'static str> {
    let _guard = unsafe { TABLE.locker.lock() };

    let cb = unsafe { &TABLE.table[idx] };
    let meta = unsafe { &mut *cb.meta.get() };
    if !meta.used {
        return Err("udp_connect: not opened");
    }

    meta.peer_addr = dst_addr.addr;
    meta.peer_port = dst_addr.port;

    if meta.port != 0 {
        return Ok(());
    }

    // bind one unused port
    'outer: for candidate in USABLE_PORT_MIN..=USABLE_PORT_MAX {
        for cb in unsafe { &TABLE.table }.iter() {
            // check already used by other CBs
            if unsafe { (*cb.meta.get()).used } && unsafe { (*cb.meta.get()).port } == candidate {
                continue 'outer;
            }
        }
        meta.port = candidate;
        break;
    }

    if meta.port == 0 {
        return Err("udp_connect: port unavailable");
    }

    Ok(())
}

pub fn send(m: Box<MBuf>, idx: usize) -> Result<(), &'static str> {
    let cb = unsafe { &TABLE.table[idx] };
    let meta = unsafe { &*cb.meta.get() };

    if !meta.used {
        return Err("udp_write: unopened");
    }

    tx(m, meta);

    Ok(())
}

#[cfg(test)]
static mut MBUFS: Vec<(Box<MBuf>, u32, Protocol)> = vec![];

/// updates the udp header and passes the packet to ip stack
fn tx(mut m: Box<MBuf>, cb: &ControlBlockMeta) {
    let hdr = m.prepend::<Header>(HEADER_SIZE);

    hdr.src_port = toggle_endian16(cb.port);
    hdr.dst_port = toggle_endian16(cb.peer_port);
    hdr.len = toggle_endian16(m.get_len() as u16);
    hdr.checksum = 0;

    #[cfg(test)]
    {
        unsafe {
            MBUFS.push((m, cb.peer_addr, Protocol::UDP));
        };
        return;
    }

    ip::tx(m, cb.peer_addr, ip::Protocol::UDP);
}

/// rx is called when packets arrives.
/// use 5-tuple to find the ControlBlock and use it.
/// enqueue the packet to the mbuf queue.
pub fn rx(mut m: Box<MBuf>, src_ip_addr: u32) -> Result<(), &'static str> {
    let hdr = m.pop::<Header>(HEADER_SIZE);

    let cb = match unsafe { &TABLE }.find(toggle_endian16(hdr.dst_port)) {
        Some(cb) => cb,
        None => {
            return Err("udp_rx: socket not found");
        }
    };

    let mut guard = cb.data.lock();
    guard.mbuf_queue.push(m);
    let slept_chan = cb.meta.get() as usize;
    drop(guard);
    unsafe { PROCESS_TABLE.wakeup(slept_chan) };

    Ok(())
}

/// dequeue the packet from the mbuf queue and copy its packet to user space
pub fn read(idx: usize, addr: usize, mut len: usize) -> Result<usize, &'static str> {
    let cb = unsafe { &TABLE.table[idx] };
    let mut guard = cb.data.lock();
    let slept_chan = cb.meta.get() as usize;
    while guard.mbuf_queue.is_empty() {
        // sleep the process while connection's mbuf queue is empty.
        guard = unsafe { CPU_TABLE.my_proc() }.sleep(slept_chan, guard);
    }

    // TODO: naive implementation
    let mut m = guard.mbuf_queue.remove(0);
    if len > m.get_len() {
        len = m.get_len();
    }

    unsafe { CPU_TABLE.my_proc() }
        .data
        .get_mut()
        .copy_out(addr, m.get_buf_head(), len)?;
    drop(guard);
    drop(m);
    Ok(len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_connect() {
        // open c0
        let c0 = open();
        assert!(c0.is_ok());
        let c0 = c0.unwrap();
        assert_eq!(0, c0);
        // open c1
        let c1 = open();
        assert!(c1.is_ok());
        let c1 = c1.unwrap();
        assert_eq!(1, c1);

        // close c0
        assert!(close(c0).is_ok());

        // when connect c0(already closed), should it returns an error
        assert_eq!(
            "udp_connect: not opened",
            connect(
                c0,
                &SockAddr {
                    family: crate::net::SAFamily::INET,
                    port: 1234,
                    addr: 1234
                }
            )
            .err()
            .unwrap()
        );

        // when connect c1, shoud it returns ok
        assert!(connect(
            c1,
            &SockAddr {
                family: crate::net::SAFamily::INET,
                port: 1234,
                addr: 1234
            }
        )
        .is_ok());
        let cb = unsafe { &mut TABLE.table[c1] };
        let cb = cb.meta.get_mut();
        assert_eq!(1234, cb.peer_addr);
        assert_eq!(1234, cb.peer_port);
        assert!(cb.port >= USABLE_PORT_MIN);
        assert!(cb.port <= USABLE_PORT_MAX);

        // when no opened, should it retunrs an error
        assert_eq!(
            "udp_connect: not opened",
            connect(
                15,
                &SockAddr {
                    family: crate::net::SAFamily::INET,
                    port: 1234,
                    addr: 1234
                }
            )
            .err()
            .unwrap()
        );

        // close c1
        assert!(close(c1).is_ok());
    }

    #[test_case]
    fn test_send() {
        // open and connect
        let c0 = open().unwrap();
        assert!(connect(
            c0,
            &SockAddr {
                family: crate::net::SAFamily::INET,
                port: 1234,
                addr: 1234
            }
        )
        .is_ok());

        let mut m = MBuf::alloc(mem::size_of::<Header>());
        send(m, c0).unwrap();

        let mut m = unsafe { MBUFS.pop() }.unwrap();

        // verify mbuf
        let hdr = unsafe { (m.0.pop(mem::size_of::<Header>()) as *const Header).as_ref() }.unwrap();
        assert_eq!(1234, toggle_endian16(hdr.dst_port));
        let src_port = unsafe { &mut TABLE.table[c0] }.meta.get_mut().port;
        assert_eq!(src_port, toggle_endian16(hdr.src_port));
        assert_eq!(mem::size_of::<Header>(), toggle_endian16(hdr.len) as usize);

        // verify peer_addr
        assert_eq!(1234, m.1);

        // verify protocol
        assert_eq!(Protocol::UDP, m.2);

        // close c0
        assert!(close(0).is_ok());
    }
}
