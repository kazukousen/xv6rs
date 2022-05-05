//! Sometimes OS kernel needs to hold a lock for a long time. for example, the file system keeps a
//! file locked while reading and writing its content on the disk, and these disk operations can
//! take tens of milliseconds. Holding a spinlock that long would lead to wasste if another process
//! wanted to acquire it, since the acquiring process would waste CPU for a long time while
//! spinning. another draw back of spinlocks is that a process cannot yield the CPU while retaining
//! a spinlock; we'd like to do this so that other processes can use the CPU while the process with
//! the lock waits for the disk.
//!
//! Yielding while holding a spinlock is illegal because it might lead to deadlock if a second
//! thread then tried to acquire the spinlock; since `lock()` doesn't yield the CPU, the second
//! thread's spinning might prevent the first thread from running and releasing the lock.
//! Yielding whinle holding a lock would also violate the requirement that interrupts must be off
//! while a spinlock is held. Thus, we'd like a type of lock that yields the CPU while waiting to
//! acquire, and allows yields (and interrupts) while the lock is held.
//!
//! Because sleep-locks leave interrupts enabled, they cannot be used tin interrput handlers.
//! Because `SleepLock<T>.lock()` may yield the CPU, sleep-locks cannot be used inside spinlock
//! critical sections (through spinlocks cannot be used inside sleep-lock critical sections).

use core::{
    cell::{Cell, UnsafeCell},
    ops::{Deref, DerefMut},
};

use crate::{cpu::CPU_TABLE, process::PROCESS_TABLE, spinlock::SpinLock};

pub struct SleepLock<T> {
    inner_lock: SpinLock<()>,
    locked: Cell<bool>,
    data: UnsafeCell<T>,
}

unsafe impl<T> Sync for SleepLock<T> {}

impl<T> SleepLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            inner_lock: SpinLock::new(()),
            locked: Cell::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SleepLockGuard<'_, T> {
        let mut guard = self.inner_lock.lock();

        while self.locked.get() {
            unsafe {
                guard = CPU_TABLE
                    .my_proc()
                    .sleep(self.locked.as_ptr() as usize, guard);
            };
        }

        self.locked.set(true);
        drop(guard);

        SleepLockGuard {
            lock: &self,
            data: unsafe { &mut (*self.data.get()) },
        }
    }

    /// called by its guard when dropped
    fn unlock(&self) {
        let guard = self.inner_lock.lock();
        self.locked.set(false);
        unsafe { PROCESS_TABLE.wakeup(self.locked.as_ptr() as usize) };
        drop(guard);
    }
}

pub struct SleepLockGuard<'a, T> {
    lock: &'a SleepLock<T>,
    data: &'a mut T,
}

impl<'a, T> Deref for SleepLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &*self.data
    }
}

impl<'a, T> DerefMut for SleepLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.data
    }
}

impl<'a, T> Drop for SleepLockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.unlock();
    }
}
