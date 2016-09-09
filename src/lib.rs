/*!
# defrag: safe and efficient memory manager for microcontrollers

This library is in the Alpha release and is subject to change

This library aims to bring safe heap memory management
to microcontrollers. Combined with rust's excellent type system and
borrow checker, creating complex applications with limited resources
is easier than it has ever been before.

The possibility of fragmentation is the primary reason that dynamic memory
allocation is not used on microcontrollers. *defrag*, as the name implies,
is able to defragment memory -- giving your embedded application power,
reliability and simplicity. Also, the overhead of each allocated block is
only 8bytes, so allocations are very cheap.

## How it works
The primary manager of memory is the `Pool`, from which the user can call
`Pool.alloc::<T>()` or `Pool.alloc_slice::<T>(len)`. From this they will get
a `Mutex<T>` like object which behaves very similarily to rust's stdlib
[`Mutex`](https://doc.rust-lang.org/std/sync/struct.Mutex.html).

When the data is not locked, the underlying pool is allowed to move it in order
to solve potential fragmentation issues. `pool.clean()` combines contiguous
free blocks and `pool.defrag()` defragments memory. In addition, there are
various strategies for utilzing freed blocks of memory.

> Note: This library is intended only for (single threaded) microcontrollers, so
> it's `Mutex` does not implement `Send` or `Sync` (it cannot be shared
> between threads). Depending on what kind of architectures or OS's spring up on
> uC rust code, this may change.
*/

#![allow(unknown_lints)]
#![feature(alloc)]
#![feature(heap_api)]
#![feature(const_fn)]
#![feature(test)]

#![no_std]

// test utilities
#[cfg(test)] extern crate test;
#[cfg(test)]
#[macro_use] extern crate std;
#[cfg(test)] use std::prelude::*;
#[cfg(test)] extern crate rand;
#[cfg(test)] extern crate stopwatch;

extern crate alloc;
extern crate cbuf;

mod types;
mod free;
mod raw_pool;
mod pool;
mod utils;

#[cfg(test)]
mod tests;

pub use types::{Error, Result};
pub use raw_pool::Block;
pub use pool::{Pool,
               Mutex, Value,
               SliceMutex, Slice};
