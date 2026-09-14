//! 100% Safe Rust reimplementation of Google V8's `src/base/platform/mutex.h`
//! and `src/base/platform/condition-variable.h`.
//!
//! Implements high-performance native synchronization primitives with recursive
//! locking support and condition variable signaling matching V8 semantics,
//! written in pure safe Rust using standard library atomic and synchronization types.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

/// Native non-recursive mutual exclusion primitive implemented with atomic spinlock.
pub struct PlatformMutex {
    locked: AtomicBool,
}

impl PlatformMutex {
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
        }
    }

    pub fn lock(&mut self) {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
    }

    pub fn unlock(&mut self) {
        self.locked.store(false, Ordering::Release);
    }

    pub fn try_lock(&mut self) -> bool {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }
}

impl Default for PlatformMutex {
    fn default() -> Self {
        Self::new()
    }
}

/// Recursive mutual exclusion primitive allowing re-entrant locking from the same thread.
pub struct PlatformRecursiveMutex {
    mutex: PlatformMutex,
    owner_thread_id: Option<std::thread::ThreadId>,
    recursion_count: u32,
}

impl PlatformRecursiveMutex {
    pub const fn new() -> Self {
        Self {
            mutex: PlatformMutex::new(),
            owner_thread_id: None,
            recursion_count: 0,
        }
    }

    pub fn lock(&mut self) {
        let current_thread = std::thread::current().id();
        if self.owner_thread_id == Some(current_thread) {
            self.recursion_count += 1;
            return;
        }

        self.mutex.lock();
        self.owner_thread_id = Some(current_thread);
        self.recursion_count = 1;
    }

    pub fn unlock(&mut self) {
        let current_thread = std::thread::current().id();
        debug_assert_eq!(self.owner_thread_id, Some(current_thread));

        self.recursion_count -= 1;
        if self.recursion_count == 0 {
            self.owner_thread_id = None;
            self.mutex.unlock();
        }
    }

    pub fn try_lock(&mut self) -> bool {
        let current_thread = std::thread::current().id();
        if self.owner_thread_id == Some(current_thread) {
            self.recursion_count += 1;
            return true;
        }

        if self.mutex.try_lock() {
            self.owner_thread_id = Some(current_thread);
            self.recursion_count = 1;
            true
        } else {
            false
        }
    }
}

impl Default for PlatformRecursiveMutex {
    fn default() -> Self {
        Self::new()
    }
}

/// Condition variable primitive compatible with PlatformMutex.
pub struct PlatformConditionVariable {
    cond: Condvar,
    lock: Mutex<bool>,
}

impl PlatformConditionVariable {
    pub const fn new() -> Self {
        Self {
            cond: Condvar::new(),
            lock: Mutex::new(false),
        }
    }

    pub fn wait(&mut self, mutex: &mut PlatformMutex) {
        mutex.unlock();
        let guard = self.lock.lock().unwrap();
        drop(self.cond.wait(guard).unwrap());
        mutex.lock();
    }

    pub fn wait_for_millis(&mut self, mutex: &mut PlatformMutex, millis: u32) -> bool {
        mutex.unlock();
        let guard = self.lock.lock().unwrap();
        let (g, timeout) = self
            .cond
            .wait_timeout(guard, Duration::from_millis(millis as u64))
            .unwrap();
        drop(g);
        mutex.lock();
        !timeout.timed_out()
    }

    pub fn notify_one(&mut self) {
        self.cond.notify_one();
    }

    pub fn notify_all(&mut self) {
        self.cond.notify_all();
    }
}

impl Default for PlatformConditionVariable {
    fn default() -> Self {
        Self::new()
    }
}
