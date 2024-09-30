# `stats_alloc_helper`

A crate that provides a helper to measure memory allocations in tests.

## Example

To allow measuring allocations, you must use the provided `LockedAllocator`,
because otherwise tests running in other thread could mess up the numbers.

Typically this means having the following at the top of section:

```rust
use std::alloc::System;
use stats_alloc::{StatsAlloc};
use stats_alloc_helper::LockedAllocator;

#[global_allocator]
static GLOBAL: LockedAllocator<System> = LockedAllocator::new(StatsAlloc::system());
```

For the tests themselves `memory_measured` is provided:

```rust
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

See crate's tests for more examples.
