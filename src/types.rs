use core;

pub type block = u16;
pub const BLOCK_NULL: block = block::max_value();
pub const BLOCK_HIGH_BIT: block = !((block::max_value() << 1) >> 1);
pub const BLOCK_BITMAP: block = !BLOCK_HIGH_BIT;

pub type index = u16;
pub const INDEX_NULL: index = index::max_value();
pub const INDEX_HIGH_BIT: index = !((index::max_value() << 1) >> 1);
pub const INDEX_BITMAP: index = !INDEX_HIGH_BIT;


/// memory error codes
#[derive(Debug, Copy, Clone)]
pub enum Error {
    /// The pool has enough memory but is too fragmented.
    /// defragment and try again.
    Fragmented,
    /// The pool has run out of memory
    OutOfMemory,
    /// The pool has run out of indexes
    OutOfIndexes,
    /// the amount of memory requested is too large
    InvalidSize,
}

pub type Result<T> = core::result::Result<T, Error>;
