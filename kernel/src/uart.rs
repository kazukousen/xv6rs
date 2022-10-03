use core::{num::Wrapping, ptr, sync::atomic::Ordering};

use crate::{
    console, cpu::CPU_TABLE, param::UART0, process::PROCESS_TABLE, spinlock::SpinLock, PANICKED,
};

const RHR: usize = 0;
const THR: usize = 0;
const IER: usize = 1;
const FCR: usize = 2;
// const ISR: usize = 2;
const LCR: usize = 3;
const LSR: usize = 5;

pub fn init() {
    unsafe {
        ptr::write_volatile((UART0 + IER) as *mut u8, 0x00);
        ptr::write_volatile((UART0 + LCR) as *mut u8, 0x80);
        ptr::write_volatile((UART0 + 0) as *mut u8, 0x03);
        ptr::write_volatile((UART0 + 1) as *mut u8, 0x00);
        ptr::write_volatile((UART0 + LCR) as *mut u8, 0x03);
        ptr::write_volatile((UART0 + FCR) as *mut u8, 0x07);
        ptr::write_volatile((UART0 + IER) as *mut u8, 0x03);
    }
}

/// alternate version of putc() that doesn't
/// use interrupts, for use by kernel printf() and
/// to echo characters. it spins waiting for the uart's
/// output register to be empty.
pub fn putc_sync(c: u8) {
    if PANICKED.load(Ordering::Relaxed) {
        loop {}
    }

    unsafe {
        while ptr::read_volatile((UART0 + LSR) as *const u8) & (1 << 5) == 0 {}
        ptr::write_volatile((UART0 + THR) as *mut u8, c);
    }
}

pub fn intr() {
    loop {
        if unsafe { ptr::read_volatile((UART0 + LSR) as *const u8) } & 1 == 0 {
            break;
        }
        let c = unsafe { ptr::read_volatile((UART0 + RHR) as *const u8) };
        console::intr(c);
    }

    let mut uart_tx = unsafe { UART_TX.lock() };
    uart_tx.start();
    drop(uart_tx);
}

const UART_TX_BUF_SIZE: usize = 32;

pub struct UartTx {
    w: usize,
    r: usize,
    buf: [u8; UART_TX_BUF_SIZE],
}

pub static mut UART_TX: SpinLock<UartTx> = SpinLock::new(
    UartTx {
        w: 0,
        r: 0,
        buf: [0; UART_TX_BUF_SIZE],
    },
    "uart",
);

impl UartTx {
    fn start(&mut self) {
        loop {
            if self.w == self.r {
                // transmit buffer is empty
                return;
            }

            if unsafe { ptr::read_volatile((UART0 + LSR) as *const u8) } & (1 << 5) == 0 {
                // the UART transmit holding register is full,
                // so we cannot give it another byte.
                // it will interrupt when it's ready for a new byte.
                return;
            }

            let r = self.r;
            let c = self.buf[r % UART_TX_BUF_SIZE];
            self.r += 1;

            // maybe putc() is waiting for space in the buffer.
            unsafe { PROCESS_TABLE.wakeup(self.r as *mut usize as usize) };

            unsafe { ptr::write_volatile((UART0 + THR) as *mut u8, c) };
        }
    }
}

impl SpinLock<UartTx> {
    pub fn putc(&self, c: u8) {
        let mut guard = self.lock();

        if PANICKED.load(Ordering::Relaxed) {
            loop {}
        }

        loop {
            if guard.w == (Wrapping(guard.r) + Wrapping(UART_TX_BUF_SIZE)).0 {
                // buffer is full
                // wait for start() to open up space in the buffer.
                guard = unsafe {
                    CPU_TABLE
                        .my_proc()
                        .sleep(guard.r as *mut usize as usize, guard)
                };
            } else {
                let w = guard.w;
                guard.buf[w % UART_TX_BUF_SIZE] = c;
                guard.w += 1;
                guard.start();
                drop(guard);
                return;
            }
        }
    }
}
