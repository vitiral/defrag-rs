#![allow(unknown_lints)]
#![feature(alloc)]
#![feature(heap_api)]
#![feature(const_fn)]

// #![no_std]
// #[cfg(test)]
// #[macro_use] extern crate std;
// #[cfg(test)] use std::prelude::*;

extern crate core;
extern crate alloc;


mod types;
mod free;
mod raw_pool;
mod pool;
mod utils;


pub use types::{Error, Result};
pub use raw_pool::{Block, RawPool};
pub use pool::{TryLockResult, TryLockError,
               Pool,
               Mutex, MutexGuard,
               SliceMutex, SliceMutexGuard};

