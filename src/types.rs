use core;

pub type BlockLoc = u16;
pub const BLOCK_NULL: BlockLoc = BlockLoc::max_value();
pub const BLOCK_HIGH_BIT: BlockLoc = !((BlockLoc::max_value() << 1) >> 1);
pub const BLOCK_BITMAP: BlockLoc = !BLOCK_HIGH_BIT;

pub type IndexLoc = u16;
pub const INDEX_NULL: IndexLoc = IndexLoc::max_value();
pub const INDEX_HIGH_BIT: IndexLoc = !((IndexLoc::max_value() << 1) >> 1);
pub const INDEX_BITMAP: IndexLoc = !INDEX_HIGH_BIT;


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
