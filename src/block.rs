//! contains the structs which actually populate the pool
//! these are the "blocks" of the pool

use super::types::*;
use super::boxed::Box;

/// the Free struct is a linked list of free values with
/// the root as a size-bin in pool
#[repr(packed)]
#[derive(Default, Copy, Clone)]
struct Free {
    // The first bit of blocks is always 0 for Free structs
    _blocks: block,        // size of this freed memory
    block: block,         // block location of this struct
    _prev: block,          // location of previous freed memory
    _next: block,          // location of next freed memory
    // data after this (until block + blocks) is invalid
}

impl Free {
    /// blocks accessor, handling any bitmaps
    fn blocks(&self) -> block {
        self._blocks
    }

    /// prev accessor, handling any bitmaps
    fn prev(&self) -> Option<block> {
        if self._prev == usize::max_value() {
            None
        } else {
            Some(self._prev)
        }
    }

    /// next accessor, handling any bitmaps
    fn next(&self) -> Option<block> {
        if self._next == usize::max_value() {
            None
        } else {
            Some(self._next)
        }
    }
}

/// the Full struct only contains enough information about the data to
/// allocate it
#[repr(packed)]
struct Full {
    // The first bit of blocks is 1 for Full structs
    _blocks: block,        // size of this freed memory
    back: *mut Box,        // pointer back to the Box wrapping this data
    // space after this (until block + blocks) is invalid
}

impl Full {
    fn blocks(&self) -> block {
        self._blocks & FULL_BITMAP
    }
}

#[test]
fn test_basic() {
    // assert that our bitmap is as expected
    let highbit = 1 << ((core::mem::size_of::<usize>() * 8) - 1);
    assert_eq!(FULL_BITMAP, !highbit, "{:b} != {:b}", FULL_BITMAP, !highbit);
    assert_eq!(HIGH_USIZE_BIT, highbit, "{:b} != {:b}", HIGH_USIZE_BIT, highbit);

    // assert that Full.blocks() cancels out the high bit
    let expected = highbit ^ usize::max_value();
    let mut f = Full {
        _blocks: usize::max_value(),
        back: 0 as *mut Box,
    };
    assert_eq!(f.blocks(), expected);
    f._blocks = highbit;
    assert_eq!(f.blocks(), 0);
    f._blocks = 0;
    assert_eq!(f.blocks(), 0);

    // assert that Free.blocks() DOESN'T cancel out the high bit (not necessary)
    let mut f = Free {
        _blocks: usize::max_value(),
        block: 0,
        _prev: 0,
        _next: 0,
    };
    assert_eq!(f.blocks(), usize::max_value());
    f._blocks = highbit;
    assert_eq!(f.blocks(), highbit);
    f._blocks = 0;
    assert_eq!(f.blocks(), 0);
}
