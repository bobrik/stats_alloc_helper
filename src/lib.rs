#![doc = include_str!("../README.md")]

use std::{
    alloc::GlobalAlloc,
    sync::atomic::{AtomicUsize, Ordering},
    thread::sleep,
    time::Duration,
};

#[cfg(feature = "async_tokio")]
use std::future::Future;

use stats_alloc::{Stats, StatsAlloc};

#[cfg(feature = "async_tokio")]
use tokio::{runtime, task::spawn_blocking};

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
    /// Returns whether the current thread locked the allocator.
    fn before_op(&self) -> bool {
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
                        return true;
                    }
                }
            }

            sleep(SLEEP);
        }

        false
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
        F: FnOnce(bool) -> O,
    {
        let locked = self.before_op();
        let result = op(locked);
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
        self.serialized(|is_locked| {
            if is_locked {
                probe::probe!(LockedAllocator, alloc_locked);
            }

            self.inner.alloc(layout)
        })
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        self.serialized(|is_locked| {
            if is_locked {
                probe::probe!(LockedAllocator, dealloc_locked);
            }

            self.inner.dealloc(ptr, layout)
        })
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        self.serialized(|is_locked| {
            if is_locked {
                probe::probe!(LockedAllocator, realloc_locked);
            }

            self.inner.realloc(ptr, layout, new_size)
        })
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

/// Measure memory and return [Stats] object for the runtime of the passed future.
#[cfg(feature = "async_tokio")]
pub async fn memory_measured_future<A, F>(alloc: &'static LockedAllocator<A>, f: F) -> Stats
where
    A: GlobalAlloc + Send + Sync,
    F: Future<Output = ()> + Send + 'static,
{
    // Tokio runtime cannot be created from a thread that is a part of a runtime already.
    spawn_blocking(|| {
        let runtime = runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            alloc.lock();

            let before = alloc.stats();

            f.await;

            let after = alloc.stats();

            alloc.unlock();

            after - before
        })
    })
    .await
    .unwrap()
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

    #[tokio::test]
    #[cfg(feature = "async_tokio")]
    async fn test_tokio() {
        let stats = memory_measured_future(&GLOBAL, async {
            let _ = vec![1, 2, 3, 4];
        })
        .await;

        assert_eq!(
            stats,
            Stats {
                allocations: 1,
                deallocations: 1,
                reallocations: 0,
                bytes_allocated: 16,
                bytes_deallocated: 16,
                bytes_reallocated: 0
            }
        );
    }
}
