//! Spinlocks protect data that is used by both threads and intterupt handlers.
//!
//! To avoid deadlock situation, if a spinlock is used by an interrupt handler, a CPU must never
//! hold that lock with interrupts enabled. So when a CPU acquires any lock, the OS kernel always
//! disables interrupts on that CPU. Intterupts may still occur on other CPUs, so an interrupt's
//! `SpinLock<T>.lock()` can wait for a thread to release a spinlock; just not on the same CPU.
//!
//! Design in Rust:
//! In xv6(C) implementation, the `lock` field in the structure is a pointer to the lock.
//! This makes it difficult to tell if the lock is locked or not, so if the developer is not
//! carefull, the data in the structure can be referenced without locking, causing a deadlock.
//!
//! Rust's type system has generics, so spin lock are designed as a smart pointer.
//! Specific data is wrapped in a lock and a guard is returned when the lock is acquired.
//! Therefore, references to data always acquire locks, and deadlock on acquisition can be avoided
//! at the compiler level.
//!
//! With Rust's drop feature, if drop is implemented on the lock, locks can be automatically
//! released when a variable goes out of scope.

use core::{
    cell::{Cell, UnsafeCell},
    ops::Deref,
    ops::DerefMut,
    sync::atomic::{fence, AtomicBool, Ordering},
};

use crate::cpu::{self, CpuTable};

pub struct SpinLock<T: ?Sized> {
    // `locked` indicates whether it is locked or not.
    locked: AtomicBool,
    name: &'static str, // for debugging
    cpu_id: Cell<isize>,
    // `data` can be mutated (or accessed exclusively) even through the spin lock itself is shared,
    // we need to use interior mutability, for which we'll use an UnsafeCell.
    data: UnsafeCell<T>,
}

// UnsafeCell does not implement Sync, which means that SpinLock<T> is now no longer shareable
// between threads, making it rather useless.
// To fix that, we need to promise to the compiler, that it is actually safe for our type to be
// shared between threads.
// However, since the lock can be used to send values of type T from one thread to another, we must
// limit this promise to types that are safe to send between threads.
// So, we implement Sync for SpinLock<T> for all T that implement Send.
unsafe impl<T> Sync for SpinLock<T> {}

impl<T> SpinLock<T> {
    pub const fn new(data: T, name: &'static str) -> Self {
        Self {
            locked: AtomicBool::new(false),
            name,
            data: UnsafeCell::new(data),
            cpu_id: Cell::new(-1),
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
        if self.holding() {
            panic!("acquire {} in cpu={}", self.name, self.cpu_id.get());
        }

        while self
            .locked
            .compare_exchange(false, true, Ordering::Release, Ordering::Acquire)
            .is_err()
        {}

        // record info about lock acquisition for holding()
        self.cpu_id.set(CpuTable::cpu_id() as isize);
    }

    fn holding(&self) -> bool {
        self.locked.load(Ordering::Acquire) && self.cpu_id.get() == CpuTable::cpu_id() as isize
    }

    fn release(&self) {
        if !self.holding() {
            panic!("release {}", self.name);
        }
        self.cpu_id.set(-1);
        self.locked.store(false, Ordering::Release);

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
