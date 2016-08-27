pub use core;

pub const NULL_BLOCK: block = usize::max_value();
pub const HIGH_BLOCK_BIT: block = !((block::max_value() << 1) >> 1);
pub const BLOCK_BITMAP: block = !HIGH_BLOCK_BIT;

pub const NULL_INDEX: index = usize::max_value();
pub const HIGH_INDEX_BIT: index = !((index::max_value() << 1) >> 1);
pub const INDEX_BITMAP: index = !HIGH_INDEX_BIT;


/// memory error codes
#[derive(Debug, Copy, Clone)]
pub enum Error {
    Fragmented,
    OutOfMemory,
    OutOfIndexes,
    InvalidSize,
}

pub type Result<T> = core::result::Result<T, Error>;
pub type block = usize;
pub type index = usize;
