# `stats_alloc_helper`

A crate that provides a helper to measure memory allocations in tests.

## Example

To allow measuring allocations, you must use the provided `LockedAllocator`,
because otherwise tests running in other thread could mess up the numbers.

Typically this means a setup similar to the following in tests:


```rust
use std::alloc::System;
use stats_alloc::{StatsAlloc, Stats};
use stats_alloc_helper::{LockedAllocator, memory_measured};

#[global_allocator]
static GLOBAL: LockedAllocator<System> = LockedAllocator::new(StatsAlloc::system());

// In the actual tests:

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
```

Async futures are supported with `async_tokio` feature enabled:

```rust,ignore
#[tokio::test]
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
```

This is achieved by creating a separate single threaded runtime
on a separate thread and driving the future to completion on it.

See crate's tests for more examples.
