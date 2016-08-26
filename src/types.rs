pub use core;

pub const HIGH_USIZE_BIT: usize = !((usize::max_value() << 1) >> 1);
pub const FULL_BITMAP: usize = !HIGH_USIZE_BIT;

/// memory error codes
pub enum Error {
    Fragmented,
    OutOfMemory,
    OutOfIndexes,
    InvalidSize,
}

pub type Result<T> = core::result::Result<T, Error>;
pub type block = usize;
