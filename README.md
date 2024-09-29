# `stats_alloc_helper`

A crate that provides a helper to measure memory allocations in tests.

## Example

To measure allocations, you must use `stats_alloc`'s allocator in your tests.
Typically this means having the following at the top of section:

```rust
use std::alloc::System;
use stats_alloc::{INSTRUMENTED_SYSTEM, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;
```

For the tests themselves [memory_measured] is provided:

```rust
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
```

See crate's tests for more examples.
