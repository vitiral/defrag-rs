#![allow(unknown_lints)]
#![feature(alloc)]
#![feature(heap_api)]
#![feature(const_fn)]

#![no_std]

// test utilities
#[cfg(test)]
#[macro_use] extern crate std;
#[cfg(test)] use std::prelude::*;
#[cfg(test)] extern crate rand;
#[cfg(test)] extern crate stopwatch;

extern crate alloc;


mod types;
mod free;
mod raw_pool;
mod pool;
mod utils;

#[cfg(test)]
mod tests;


pub use types::{Error, Result};
pub use raw_pool::{Block, RawPool};
pub use pool::{TryLockResult, TryLockError,
               Pool,
               Mutex, MutexGuard,
               SliceMutex, SliceMutexGuard};

