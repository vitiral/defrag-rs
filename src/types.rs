pub use core;

pub const BLOCK_NULL: block = index::max_value();
pub const BLOCK_HIGH_BIT: block = !((block::max_value() << 1) >> 1);
pub const BLOCK_BITMAP: block = !BLOCK_HIGH_BIT;

pub const INDEX_NULL: index = index::max_value();
pub const INDEX_HIGH_BIT: index = !((index::max_value() << 1) >> 1);
pub const INDEX_BITMAP: index = !INDEX_HIGH_BIT;


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
