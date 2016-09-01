#![allow(unknown_lints)]
#![feature(const_fn)]

// #![no_std]
// #[cfg(test)]
// #[macro_use] extern crate std;
// #[cfg(test)] use std::prelude::*;

extern crate core;


mod types;
mod free;
mod pool;
mod mutex;
mod utils;


pub use types::{Error, Result};
pub use pool::{Block, RawPool};
pub use mutex::{TryLockResult, TryLockError,
                Pool,
                Mutex, MutexGuard,
                SliceMutex, SliceMutexGuard};

