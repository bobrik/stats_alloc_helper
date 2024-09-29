//! # `stats_alloc_helper`
//!
//! A crate that provides a helper to measure memory allocations in tests.
//!
//! ## Example
//!
//! To measure allocations, you must use `stats_alloc`'s allocator in your tests.
//! Typically this means having the following at the top of section:
//!
//! ```
//! use std::alloc::System;
//! use stats_alloc::{INSTRUMENTED_SYSTEM, StatsAlloc};
//!
//! #[global_allocator]
//! static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;
//! ```
//!
//! For the tests themselves [memory_measured] is provided:
//!
//! ```
//! # use std::alloc::System;
//! # use stats_alloc::{INSTRUMENTED_SYSTEM, Stats, StatsAlloc};
//! # use stats_alloc_helper::memory_measured;
//! #
//! # #[global_allocator]
//! # static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;
//! #
//! let mut length = 0;
//!
//! let stats = memory_measured(GLOBAL, || {
//!   let v = vec![1, 2, 3, 4, 5];
//!
//!   length = v.len();
//! });
//!
//! assert_eq!(length, 5);
//!
//! assert_eq!(
//!   stats,
//!   Stats {
//!    allocations: 1,
//!    deallocations: 1,
//!    reallocations: 0,
//!    bytes_allocated: 20,
//!    bytes_deallocated: 20,
//!    bytes_reallocated: 0
//!  }
//! );
//! ```
//!
//! See crate's tests for more examples.

use std::{alloc::GlobalAlloc, sync::Mutex};

use stats_alloc::{Stats, StatsAlloc};

static MEMORY_MEASUREMENT_MUTEX: Mutex<()> = Mutex::new(());

/// Measure memory and return [Stats] object for the runtime of the passed closure.
pub fn memory_measured<A, F>(alloc: &StatsAlloc<A>, mut f: F) -> Stats
where
    A: GlobalAlloc,
    F: FnMut(),
{
    let guard = MEMORY_MEASUREMENT_MUTEX.lock().unwrap();
    let before = alloc.stats();

    f();

    let after = alloc.stats();

    drop(guard);

    after - before
}

#[cfg(test)]
mod tests {
    use std::alloc::System;

    use stats_alloc::INSTRUMENTED_SYSTEM;

    use super::*;

    #[global_allocator]
    static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

    #[test]
    fn it_works() {
        let mut length = 0;

        let stats = memory_measured(GLOBAL, || {
            let v = vec![1, 2, 3, 4, 5];

            length = v.len();
        });

        assert_eq!(length, 5);

        assert_eq!(
            stats,
            Stats {
                allocations: 1,
                deallocations: 1,
                reallocations: 0,
                bytes_allocated: 20,
                bytes_deallocated: 20,
                bytes_reallocated: 0
            }
        );

        let stats = memory_measured(GLOBAL, || {
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
}
