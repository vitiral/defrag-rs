#![no_std]
#![feature(const_fn)]

#[cfg(test)]
#[macro_use] extern crate std;
#[cfg(test)] use std::prelude::*;

mod types;
mod pool;
mod mutex;
