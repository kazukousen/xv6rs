//! Spinlocks protect data that is used by both threads and intterupt handlers.
//!
//! To avoid deadlock situation, if a spinlock is used by an interrupt handler, a CPU must never
//! hold that lock with interrupts enabled. So when a CPU acquires any lock, the OS kernel always
//! disables interrupts on that CPU. Intterupts may still occur on other CPUs, so an interrupt's
//! `SpinLock<T>.lock()` can wait for a thread to release a spinlock; just not on the same CPU.

use core::{
    cell::UnsafeCell,
    ops::Deref,
    ops::DerefMut,
    sync::atomic::{fence, AtomicBool, Ordering},
};

use crate::cpu;

pub struct SpinLock<T: ?Sized> {
    lock: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: ?Sized + Send> Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }
}

impl<T: ?Sized> SpinLock<T> {
    pub fn lock(&self) -> SpinLockGuard<'_, T> {
        self.acquire();
        SpinLockGuard {
            inner: &self,
            data: unsafe { &mut *self.data.get() },
        }
    }

    fn acquire(&self) {
        // disable interrupts to avoid deadlock.
        cpu::push_off();

        while self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Acquire)
            .is_err()
        {}
        fence(Ordering::SeqCst);
    }

    fn release(&self) {
        fence(Ordering::SeqCst);
        self.lock.store(false, Ordering::Release);

        cpu::pop_off();
    }

    pub fn unlock(&self) {
        self.release();
    }
}

pub struct SpinLockGuard<'a, T: ?Sized> {
    inner: &'a SpinLock<T>,
    data: &'a mut T,
}

impl<'a, T: ?Sized> SpinLockGuard<'a, T> {
    pub fn weak(self) -> SpinLockWeakGuard<'a, T> {
        SpinLockWeakGuard { inner: self.inner }
    }
}

impl<'a, T: ?Sized> Deref for SpinLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &*self.data
    }
}

impl<'a, T: ?Sized> DerefMut for SpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.data
    }
}

impl<'a, T: ?Sized> Drop for SpinLockGuard<'a, T> {
    fn drop(&mut self) {
        self.inner.release();
    }
}

pub struct SpinLockWeakGuard<'a, T: ?Sized> {
    inner: &'a SpinLock<T>,
}

impl<'a, T: ?Sized> SpinLockWeakGuard<'a, T> {
    pub fn lock(self) -> SpinLockGuard<'a, T> {
        self.inner.lock()
    }
}
