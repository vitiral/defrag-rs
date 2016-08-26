//! contains all the logic related to the pool
//! these are the "blocks" of the pool

use super::types::*;
use super::boxed::Box;

pub const FREE_NONE: usize = usize::max_value();

struct Pool {
    _freed: block,
    heap_block: block,
    total_used: block,
    blocks: [Free; 4096],
}

impl Pool {
    fn new() -> Pool {
        Pool {
            _freed: FREE_NONE,
            heap_block: 0,
            total_used: 0,
            blocks: [Free::default(); 4096],
        }
    }

    fn freed(&self) -> Option<block> {
        if self._freed == FREE_NONE {
            None
        } else {
            Some(self._freed)
        }
    }

    fn get_freed(&mut self, block: Option<block>) -> Option<&mut Free> {
        match block {
            Some(b) => {
                assert!(b < self.blocks.len() - 1);
                Some(&mut self.blocks[b as usize])
            }
            None => None,
        }
    }

    fn set_freed(&mut self, free: Option<&mut Free>) {
        match free {
            Some(f) => {
                self._freed = f.block;
                f._prev = FREE_NONE;  // it is the beginning of the list
            }
            None => self._freed = FREE_NONE,
        }
    }

    fn get_freed_block(&mut self, blocks: block) -> Option<block> {
        let mut block = match self.freed() {
            Some(b) => b,
            None => return None,
        };
        loop {
            let freed = &mut self.blocks[block as usize] as *mut Free;
            unsafe {
                // all unsafe operations are safe because we know that we are
                // never changing more than one freed block at a time
                if (*freed).blocks() == blocks {
                    // perfectly equal, consumes freed block
                    (*freed).remove(self);
                    return Some((*freed).block);
                } else if (*freed).blocks() > blocks {
                    // use only the size that is needed, so append a new freed block
                    let new_freed = &mut self.blocks[((*freed).block + blocks) as usize]
                        as *mut Free;
                    (*freed).append(self, &mut (*new_freed));
                    (*freed).remove(self);
                    return Some((*freed).block);
                }
                block = match (*freed).next() {
                    Some(b) => b,
                    None => return None,
                };
            }
        }
    }
}

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
        if self._prev == FREE_NONE {
            None
        } else {
            Some(self._prev)
        }
    }

    /// next accessor, handling any bitmaps
    fn next(&self) -> Option<block> {
        if self._next == FREE_NONE {
            None
        } else {
            Some(self._next)
        }
    }

    unsafe fn set_next(&mut self, next: Option<&mut Free>) {
        match next {
            Some(n) => {
                self._next = n.block;
                n._prev = self.block;
            }
            None => self._next = FREE_NONE,
        }
    }

    /// set the prev freed block and set it's next to self
    unsafe fn set_prev(&mut self, pool: &mut Pool, prev: Option<&mut Free>) {
        match prev {
            Some(p) => {
                self._prev = p.block;
                p._next = self.block;
            }
            None => {
                pool.set_freed(Some(self));
            }
        }
    }

    /// append a freed block after this one
    fn append(&mut self, pool: &mut Pool, next: &mut Free) {
        let pool = pool as *mut Pool;
        unsafe {
            if let Some(n) = self.next() {
                (*pool).blocks[n as usize].set_prev(&mut (*pool), Some(next));
            }
            self.set_next(Some(next));
        }
    }

    /// remove self from the freed pool
    fn remove(&mut self, pool: &mut Pool) {
        let poolp = pool as *mut Pool;
        unsafe {
            match (*poolp).get_freed(self.prev()) {
                Some(p) => p.set_next(pool.get_freed(self.next())),
                None => (*poolp).set_freed(pool.get_freed(self.next())),
            }
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
