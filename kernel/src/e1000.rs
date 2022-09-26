use core::{
    convert::TryInto,
    mem, ptr,
    sync::atomic::{fence, Ordering},
};

use alloc::boxed::Box;
use array_macro::array;

use crate::{mbuf::MBuf, net, param::E1000_REGS_ADDR, process::PROCESS_TABLE, spinlock::SpinLock};

#[repr(C, align(16))]
struct TxDesc {
    addr: u64,
    length: u16,
    cso: u8,
    cmd: u8,
    status: u8,
    css: u8,
    special: u16,
}

impl TxDesc {
    const fn new() -> Self {
        Self {
            addr: 0,
            length: 0,
            cso: 0,
            cmd: 0,
            status: 0,
            css: 0,
            special: 0,
        }
    }
}

#[repr(C, align(16))]
struct RxDesc {
    // The hardware always consumes descriptors from the head and moves the head pointer.
    addr: u64,
    length: u16,
    csum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

impl RxDesc {
    const fn new() -> Self {
        Self {
            addr: 0,
            length: 0,
            csum: 0,
            status: 0,
            errors: 0,
            special: 0,
        }
    }
}

const TX_RING_SIZE: usize = 16;
const RX_RING_SIZE: usize = 16;

const E1000_ICR: u32 = 0x000C0; // Interrupt Cause Read
const E1000_IMS: u32 = 0x000d0;
const E1000_TCTL: u32 = 0x0400;
const E1000_RCTL: u32 = 0x0100;
const E1000_TIPG: u32 = 0x0410;

const E1000_RDBAL: u32 = 0x02800; // RX Descriptor Base Address Low
const E1000_RDTR: u32 = 0x02820; // Delay Timer
const E1000_RADV: u32 = 0x0282C; // RX Interrupt Absolute Delay Timer
const E1000_RDH: u32 = 0x02810; // RX Descriptor Head
const E1000_RDT: u32 = 0x02818; // RX Descriptor Tail
const E1000_RDLEN: u32 = 0x02808; // RX Descriptor Length
const E1000_RSRPD: u32 = 0x02c00; // RX Small Packet Detect Interrupt
const E1000_TDBAL: u32 = 0x03800; // TX Descriptor Base Address Low
const E1000_TDLEN: u32 = 0x03808; // TX Descriptor Length
const E1000_TDH: u32 = 0x03810; // TX Descriptor Head
const E1000_TDT: u32 = 0x03818; // TX Descriptor Tail
const E1000_RA: u32 = 0x05400; // Receive Address

const E1000_TXD_STAT_DD: u8 = 0x00000001; // Descriptor Done
const E1000_TXD_CMD_EOP: u8 = 0x01; // End of Packet
const E1000_TXD_CMD_RS: u8 = 0x08; // End of Packet
const E1000_RXD_STAT_DD: u8 = 0x01; // Descriptor Done
const E1000_RXD_STAT_EOP: u8 = 0x02; // End of Packet

const DATA_MAX: usize = 1518;

/// these queues provide pointers to memory location for the DMA.
#[repr(C, align(16))]
pub struct E1000 {
    tx_ring: [TxDesc; TX_RING_SIZE],
    tx_mbufs: [Option<Box<MBuf>>; TX_RING_SIZE],
    rx_ring: [RxDesc; RX_RING_SIZE],
    rx_mbufs: [Option<Box<MBuf>>; RX_RING_SIZE],
}

pub static E1000: SpinLock<E1000> = SpinLock::new(E1000::new(), "e1000");
// unsafe impl Send for E1000 {}

impl E1000 {
    const fn new() -> Self {
        Self {
            tx_ring: array![_ => TxDesc::new(); TX_RING_SIZE],
            tx_mbufs: array![_ => None; TX_RING_SIZE],
            rx_ring: array![_ => RxDesc::new(); RX_RING_SIZE],
            rx_mbufs: array![_ => None; RX_RING_SIZE],
        }
    }

    /// Configures the E1000 to read packets to be transmitted from RAM, and to write received
    /// packets to RAM. This technique is called DMA (Direct Memory Access), referring to the fact
    /// that the E1000 hardware directory writes and reads packets to/from RAM.
    /// 1. Reset the device
    /// 2. [E1000 14.5] Transmit initialization
    /// 3. [E1000 14.4] Receive initialization
    /// 4. transmitter, receiver control bits
    /// 5. Ask E1000 for receive interrupts
    pub fn init(&mut self) {
        // Reset the device
        write_e1000_regs(E1000_IMS, 0); // disable interrupts

        self.tx_init();
        self.rx_init();

        // MAC address of qemu (52:54:00:12:34:56)
        write_e1000_regs(E1000_RA, 0x12005452);
        write_e1000_regs(E1000_RA + 8, 0x10005634);
        // mask for receive timer interrupt
        write_e1000_regs(E1000_TCTL, (0x40 << 12) | (0x10 << 4) | (1 << 3) | (1 << 1)); // transmitter control bits
        write_e1000_regs(E1000_TIPG, 10 | (8 << 10) | (6 << 20));
        write_e1000_regs(E1000_RCTL, (0x4 << 24) | (0x8 << 12) | (1 << 1)); // reveiver control bits
        write_e1000_regs(E1000_RDTR, 0); // interrupt after every received packet (no timer)
        write_e1000_regs(E1000_RADV, 0); // interrupt after every packet (no timer)
        write_e1000_regs(E1000_IMS, 1 << 7);
    }

    fn tx_init(&mut self) {
        for i in 0..TX_RING_SIZE {
            self.tx_ring[i].status = E1000_TXD_STAT_DD;
        }
        write_e1000_regs(E1000_TDBAL, &self.tx_ring as *const _ as u32);
        write_e1000_regs(E1000_TDLEN, mem::size_of::<[TxDesc; TX_RING_SIZE]>() as u32);
        write_e1000_regs(E1000_TDT, 0);
        write_e1000_regs(E1000_TDH, 0);
        write_e1000_regs(E1000_TCTL, (0x40 << 12) | (0xf << 4) | (1 << 3) | (1 << 1));

        // init tx descriptors
        // set RS(bit 3) and EOP(bit 1) to 1.
        let cmd: u8 = 0b1001;
        for i in 0..TX_RING_SIZE {
            self.tx_ring[i].cmd = cmd;
        }
    }

    fn rx_init(&mut self) {
        write_e1000_regs(E1000_RDT, RX_RING_SIZE as u32);

        for i in 0..RX_RING_SIZE {
            let mut m = MBuf::alloc(0);
            self.rx_ring[i].addr = m.get_buf_head() as u64;
            self.rx_mbufs[i].replace(m);
        }

        write_e1000_regs(E1000_RDBAL, &self.rx_ring as *const _ as u32);
        write_e1000_regs(E1000_RDLEN, mem::size_of::<[RxDesc; RX_RING_SIZE]>() as u32);
        write_e1000_regs(E1000_RDT, RX_RING_SIZE as u32 - 1);
        write_e1000_regs(E1000_RDH, 0);
    }

    pub fn send(&mut self, mut m: Box<MBuf>) -> Result<(), &str> {
        // For transmitting, first get the current ring position, using E1000_TDT.
        let pos = read_e1000_regs(E1000_TDT) as usize;

        // Then check if the ring is overflowing. If E1000_TXD_STAT_DD is not set in the current
        // descriptor, a previous transmittion in flight, so return an error.
        let tail = &mut self.tx_ring[pos];
        if tail.status & E1000_TXD_STAT_DD == 0 {
            return Err("a previous transmittion is still in flight");
        }

        tail.status = 0;
        // provide the new mbuf's head pointer and length.
        tail.addr = m.get_buf_head() as u64;
        tail.length = m.get_len() as u16;
        // set the necessary cmd flags
        tail.cmd = E1000_TXD_CMD_EOP | E1000_TXD_CMD_RS;

        // stash away the pointer to the new mbuf for later freeing.
        let old = self.tx_mbufs[pos].replace(m);
        // Free the last mbuf that was transmitted with the current descriptor (if there was one).
        old.map(|old| drop(old));

        // Finally, update the ring position by adding one to E1000_TDT modulo TX_RING_SIZE.
        // Tell hardware there is a new ready-to-send packet, by move down the tail position.
        write_e1000_regs(E1000_TDT, ((pos + 1) % TX_RING_SIZE) as u32);
        fence(Ordering::SeqCst);

        Ok(())
    }
}

#[inline]
fn write_e1000_regs(i: u32, v: u32) {
    let offset = E1000_REGS_ADDR + i;
    unsafe { ptr::write_volatile(offset as *mut u32, v) };
}

#[inline]
fn read_e1000_regs(i: u32) -> u32 {
    let offset = E1000_REGS_ADDR + i;
    let ret = unsafe { ptr::read_volatile(offset as *mut u32) };
    ret
}

impl SpinLock<E1000> {
    /// The E1000 generates an interrupt whenever new packets are received.
    /// So we scan the RX queue to handle each packet and deliver its mbuf to the protocol layer.
    pub fn intr(&self) {
        // tell the e1000 we've seen this interrupt
        // without this the e1000 won't raise any
        // further interrupts.
        self.recv();
        read_e1000_regs(E1000_ICR);
        unsafe { PROCESS_TABLE.wakeup(self as *const _ as usize) };
    }

    fn recv(&self) {
        // First get the next ring position, using E1000_RDT plus one modulo RX_RING_SIZE.
        let mut pos = (read_e1000_regs(E1000_RDT) + 1) as usize % RX_RING_SIZE;
        loop {
            let mut guard = self.lock();
            let tail = &guard.rx_ring[pos];
            // Then check if a new packet is available by checking for the E1000_RXD_STAT_DD bit in
            // the status portion of the descriptor. If not, stop.
            if tail.status & E1000_RXD_STAT_DD == 0 {
                drop(guard);
                break;
            }

            let mut new_m = MBuf::alloc(0);
            let new_m_ptr = new_m.get_buf_head() as u64;

            // Pass a new mbuf's (which is allocated to replace the one just derivered to network stack)
            // data pointer into the descriptor.
            let mut m = guard.rx_mbufs[pos].replace(new_m).unwrap();

            // Update m.len to the length reported in the descriptor.
            let tail = &mut guard.rx_ring[pos];
            m.append(tail.length.into());

            tail.addr = new_m_ptr as u64;
            // Clear the discriptor's status bits to zero.
            tail.status = 0;

            // Finally, update the E1000_RDT register to be the position of the last ring
            // descriptor processed.
            write_e1000_regs(E1000_RDT, pos as u32);
            drop(guard);

            // deriver the mbuf to network stack
            net::rx(m);

            pos = (read_e1000_regs(E1000_RDT) + 1) as usize % RX_RING_SIZE;
        }
    }
}
