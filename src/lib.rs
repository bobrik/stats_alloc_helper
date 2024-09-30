//! # `stats_alloc_helper`
//!
//! A crate that provides a helper to measure memory allocations in tests.
//!
//! ## Example
//!
//! To allow measuring allocations, you must use the provided [LockedAllocator],
//! because otherwise tests running in other thread could mess up the numbers.
//!
//! Typically this means having the following at the top of section:
//!
//! ```
//! use std::alloc::System;
//! use stats_alloc::{StatsAlloc};
//! use stats_alloc_helper::LockedAllocator;
//!
//! #[global_allocator]
//! static GLOBAL: LockedAllocator<System> = LockedAllocator::new(StatsAlloc::system());
//! ```
//!
//! For the tests themselves [memory_measured] is provided:
//!
//! ```
//! # use std::alloc::System;
//! # use stats_alloc::{Stats, StatsAlloc};
//! # use stats_alloc_helper::{LockedAllocator, memory_measured};
//! #
//! # #[global_allocator]
//! # static GLOBAL: LockedAllocator<System> = LockedAllocator::new(StatsAlloc::system());
//! #
//! let mut length = 0;
//!
//! let stats = memory_measured(&GLOBAL, || {
//!     let s = "whoa".to_owned().replace("whoa", "wow").to_owned();
//!
//!     length = s.len();
//! });
//!
//! assert_eq!(length, 3);
//!
//! assert_eq!(
//!     stats,
//!     Stats {
//!         allocations: 3,
//!         deallocations: 3,
//!         reallocations: 0,
//!         bytes_allocated: 15,
//!         bytes_deallocated: 15,
//!         bytes_reallocated: 0
//!     }
//! );
//! ```
//!
//! See crate's tests for more examples.

use std::{
    alloc::GlobalAlloc,
    sync::atomic::{AtomicUsize, Ordering},
    thread::sleep,
    time::Duration,
};

use stats_alloc::{Stats, StatsAlloc};

const STATE_UNLOCKED: usize = 0;
const STATE_IN_USE: usize = 1;

const SLEEP: Duration = Duration::from_micros(50);

pub struct LockedAllocator<T>
where
    T: GlobalAlloc,
{
    locked: AtomicUsize,
    inner: StatsAlloc<T>,
}

impl<T> LockedAllocator<T>
where
    T: GlobalAlloc,
{
    pub const fn new(inner: StatsAlloc<T>) -> Self {
        let locked = AtomicUsize::new(0);
        Self { locked, inner }
    }

    /// An allocation free way to get the current thread id.
    fn current_thread_id() -> usize {
        unsafe { libc::pthread_self() as usize }
    }

    /// An allocation free serialization code that runs prior to any allocator operation.
    fn before_op(&self) {
        let current_thread_id = Self::current_thread_id();

        loop {
            match self.locked.compare_exchange(
                STATE_UNLOCKED,
                STATE_IN_USE,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(existing) => {
                    if existing == current_thread_id {
                        break;
                    }
                }
            }

            sleep(SLEEP);
        }
    }

    /// An allocation free serialization code that runs after to any allocator operation.
    fn after_op(&self) {
        let current_thread_id = Self::current_thread_id();

        loop {
            match self.locked.compare_exchange(
                STATE_IN_USE,
                STATE_UNLOCKED,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(existing) => {
                    if existing == current_thread_id {
                        break;
                    }
                }
            }

            sleep(SLEEP);
        }
    }

    /// A serialization wrapper to use for all allocator operations.
    fn serialized<F, O>(&self, op: F) -> O
    where
        F: FnOnce() -> O,
    {
        self.before_op();
        let result = op();
        self.after_op();

        result
    }

    /// Lock the allocator to only allow operations from the current thread.
    fn lock(&self) {
        let current_thread_id = Self::current_thread_id();

        loop {
            let r = self.locked.compare_exchange(
                STATE_UNLOCKED,
                current_thread_id,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );

            if r.is_ok() {
                break;
            }

            sleep(SLEEP);
        }
    }

    /// Unlocks the allocator to allow operations from any thread.
    fn unlock(&self) {
        let expected = Self::current_thread_id();

        assert_eq!(
            expected,
            self.locked
                .compare_exchange(expected, STATE_UNLOCKED, Ordering::SeqCst, Ordering::SeqCst)
                .unwrap()
        );
    }

    /// Returns [Stats] from the wrapped [StatsAlloc].
    fn stats(&self) -> Stats {
        self.inner.stats()
    }
}

unsafe impl<T> GlobalAlloc for LockedAllocator<T>
where
    T: GlobalAlloc,
{
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        self.serialized(|| self.inner.alloc(layout))
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        self.serialized(|| self.inner.dealloc(ptr, layout))
    }

    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        self.serialized(|| self.inner.alloc_zeroed(layout))
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        self.serialized(|| self.inner.realloc(ptr, layout, new_size))
    }
}

/// Measure memory and return [Stats] object for the runtime of the passed closure.
pub fn memory_measured<A, F>(alloc: &LockedAllocator<A>, f: F) -> Stats
where
    A: GlobalAlloc,
    F: FnOnce(),
{
    alloc.lock();

    let before = alloc.stats();

    f();

    let after = alloc.stats();

    alloc.unlock();

    after - before
}

#[cfg(test)]
mod tests {
    use std::{
        alloc::System,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread::{sleep, spawn},
        time::Duration,
    };

    use super::*;

    #[global_allocator]
    static GLOBAL: LockedAllocator<System> = LockedAllocator::new(StatsAlloc::system());

    #[test]
    fn it_works() {
        let mut length = 0;

        let stats = memory_measured(&GLOBAL, || {
            let s = "whoa".to_owned().replace("whoa", "wow").to_owned();

            length = s.len();
        });

        assert_eq!(length, 3);

        assert_eq!(
            stats,
            Stats {
                allocations: 3,
                deallocations: 3,
                reallocations: 0,
                bytes_allocated: 15,
                bytes_deallocated: 15,
                bytes_reallocated: 0
            }
        );

        let stats = memory_measured(&GLOBAL, || {
            let mut v = vec![1, 2, 3, 4, 5];

            v.push(6);

            length = v.len();
        });

        assert_eq!(length, 6);

        assert_eq!(
            stats,
            Stats {
                allocations: 1,
                deallocations: 1,
                reallocations: 1,
                bytes_allocated: 40,
                bytes_deallocated: 40,
                bytes_reallocated: 20
            }
        );
    }

    #[test]
    fn test_parallel() {
        let stop = Arc::new(AtomicBool::new(false));

        {
            let stop = stop.clone();
            spawn(move || {
                let mut vec = vec![];
                while !stop.load(Ordering::Relaxed) {
                    vec.push(1);
                    sleep(Duration::from_micros(1));
                }
            });
        }

        let mut length = 0;
        let step = Duration::from_millis(150);

        let stats = memory_measured(&GLOBAL, || {
            let s = "whoa".to_owned().replace("whoa", "wow").to_owned();

            sleep(step);

            length = s.len();
        });

        stop.store(true, Ordering::Relaxed);

        assert_eq!(length, 3);

        assert_eq!(
            stats,
            Stats {
                allocations: 3,
                deallocations: 3,
                reallocations: 0,
                bytes_allocated: 15,
                bytes_deallocated: 15,
                bytes_reallocated: 0
            }
        );
    }
}
