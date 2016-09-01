use core::mem;
use core::default::Default;
use core::fmt;

use super::types::*;
use super::pool::{RawPool, Block, BlockType};

// ##################################################
// # Free

/// the Free struct is a linked list of free values with
/// the root as a size-bin in pool
#[repr(C, packed)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct Free {
    // NOTE: DO NOT MOVE `_blocks`, IT IS SWAPPED WITH `Full._blocks`
    // The first bit of `_blocks` is always 0 for Free structs
    pub _blocks: block,        // size of this freed memory
    pub _block: block,          // block location of this struct
    pub _prev: block,          // location of previous freed memory
    pub _next: block,          // location of next freed memory
    // data after this (until block + blocks) is invalid
}

impl fmt::Debug for Free {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let prev: isize = if self._prev == BLOCK_NULL {
            -1
        } else {
            self._prev as isize
        };
        let next = if self._next == BLOCK_NULL {
            -1
        } else {
            self._next as isize
        };
        let isvalid = if self.is_valid() {" "} else {"!"};
        write!(f, "Free{}{{block: {}, blocks: {}, prev: {}, next: {}}}{}",
               isvalid,
               self._block & BLOCK_BITMAP,
               self._blocks & BLOCK_BITMAP,
               prev, next, isvalid)
    }
}

impl Default for Free {
    fn default() -> Free {
        Free {
            _blocks: 0,
            _block: BLOCK_NULL,
            _prev: BLOCK_NULL,
            _next: BLOCK_NULL,
        }
    }
}

/// Free is a private struct to defrag, so all accessor
impl Free {
    // public accessors (public for tests)

    /// block accessor
    pub fn block(&self) -> block {
        self.assert_valid();
        self._block
    }

    /// blocks accessor, handling any bitmaps
    pub fn blocks(&self) -> block {
        self.assert_valid();
        self._blocks
    }

    /// prev accessor, handling any bitmaps
    pub fn prev(&self) -> Option<block> {
        self.assert_valid();
        if self._prev == BLOCK_NULL {
            None
        } else {
            Some(self._prev)
        }
    }

    /// next accessor, handling any bitmaps
    pub fn next(&self) -> Option<block> {
        self.assert_valid();
        if self._next == BLOCK_NULL {
            None
        } else {
            Some(self._next)
        }
    }

    // private methods for manipulating the Free linked-list

    /// set the prev freed block and set it's next to self
    pub unsafe fn set_prev(&mut self, pool: &mut RawPool, prev: Option<&mut Free>) {
        self.assert_valid();
        match prev {
            Some(p) => {
                self._prev = p.block();
                p._next = self.block();
            }
            None => {
                let pool = pool as *mut RawPool;
                (*pool).freed_bins.insert(&mut *pool, self);
            }
        }
    }

    /// set the next freed block and set it's prev to self
    pub unsafe fn set_next(&mut self, next: Option<&mut Free>) {
        self.assert_valid();
        match next {
            Some(n) => {
                self._next = n.block();
                n._prev = self.block();
            }
            None => self._next = BLOCK_NULL,
        }
    }

    /// append a freed block after this one
    unsafe fn append(&mut self, pool: &mut RawPool, next: &mut Free) {
        self.assert_valid();
        let pool = pool as *mut RawPool;
        if let Some(n) = self.next() {
            // set prev of the next freed block (if it exists)
            (*pool).freed_mut(n).set_prev(&mut (*pool), Some(next));
        }
        self.set_next(Some(next));
    }

    /// remove self from the freed pool
    /// this also keeps track of the statistics for number of freed blocks
    unsafe fn remove(&mut self, pool: &mut RawPool) {
        self.assert_valid();
        /// convinience function for this method only
        unsafe fn get_freed<'a>(pool: &'a mut RawPool, block: Option<block>) -> Option<&'a mut Free> {
            match block {
                Some(b) => {
                    assert!(b < pool.len_blocks() - 1);
                    Some(pool.freed_mut(b))
                }
                None => None,
            }
        }
        pool.freed_bins.len -= 1;
        let poolp = pool as *mut RawPool;
        match get_freed(&mut *poolp, self.prev()) {
            Some(p) => p.set_next(get_freed(&mut *poolp, self.next())),
            None => {
                // it is the first item in a bin so it needs to remove itself
                let bin = (*poolp).freed_bins.get_insert_bin(self.blocks());
                (*poolp).freed_bins.bins[bin as usize]._root = self._next;
            }
        }
    }

    /// join two freed values together, assumes they are right next to eachother
    /// returns the new freed value
    pub unsafe fn join(&mut self, pool: &mut RawPool, right: &mut Free) -> &mut Free {
        self.assert_valid();
        right.assert_valid();
        // remove them both from any bins -- their combined bin might change
        // anyway
        self.remove(pool);
        right.remove(pool);
        self._blocks += right.blocks();
        self.assert_valid();
        (*(pool as *mut RawPool)).freed_bins.insert(pool, self);
        self
    }

    fn assert_valid(&self) {
        assert!(self.is_valid(), "{:?}", self);
    }

    fn is_valid(&self) -> bool {
        self._blocks & BLOCK_HIGH_BIT == 0 && self._blocks != 0
    }
}


// ##################################################
// # Freed Bins and Root

/// a FreedRoot stores the beginning of the linked list
/// and keeps track of statistics
#[repr(C, packed)]
pub struct FreedRoot {
    pub _root: block,
}

impl Default for FreedRoot {
    fn default() -> FreedRoot {
        FreedRoot {_root: BLOCK_NULL}
    }
}

impl FreedRoot {
    /// public for tests to access
    pub unsafe fn root<'a>(&self, pool: &'a RawPool) -> Option<&'a Free> {
        if self._root == BLOCK_NULL {
            None
        } else {
            Some(pool.freed(self._root))
        }
    }

    unsafe fn root_mut<'a>(&mut self, pool: &'a mut RawPool) -> Option<&'a mut Free> {
        if self._root == BLOCK_NULL {
            None
        } else {
            Some(pool.freed_mut(self._root))
        }
    }

    unsafe fn insert_root(&mut self, pool: &mut RawPool, freed: &mut Free) {
        freed._prev = BLOCK_NULL;
        if let Some(cur_root) = self.root_mut(&mut *(pool as *mut RawPool)) {
            cur_root.set_prev(pool, Some(freed));
        } else {
            freed._next = BLOCK_NULL;
        }
        self._root = freed.block();
    }

}

pub const NUM_BINS: u8 = 7;

/// the FreedBins provide simple and fast access to freed data
/// FreedBins is a private struct so all accessors are pub
#[derive(Default)]
pub struct FreedBins {
    pub len: block,
    pub bins: [FreedRoot; NUM_BINS as usize],
}

impl FreedBins {
    /// get the bin that would be used when
    /// inserting a Free value
    pub fn get_insert_bin(&self, blocks: block) -> u8 {
        match blocks {
            1   ...3     => 0,
            4   ...15    => 1,
            16  ...63    => 2,
            63  ...255   => 3,
            256 ...1023  => 4,
            1024...4095  => 5,
            _            => 6,
        }
    }

    pub fn bin_repr(bin: u8) -> &'static str {
        match bin {
            0 => "1   ...3   ",
            1 => "4   ...15  ",
            2 => "16  ...63  ",
            3 => "63  ...255 ",
            4 => "256 ...1023",
            5 => "1024...4095",
            6 => "4096...    ",
            _ => "INVALID",
        }
    }

    /// insert a Free block into a freed bin
    /// this is the only method that Pool uses to store deallocated indexes
    pub unsafe fn insert(&mut self, pool: &mut RawPool, freed: &mut Free) {
        assert!(freed.block() < pool.heap_block);
        assert!(freed.blocks() < pool.blocks_used);
        self.len += 1;
        let bin = self.get_insert_bin(freed.blocks());
        self.bins[bin as usize].insert_root(pool, freed);
    }

    /// get a block of the requested size from the freed bins, removing it from the freed
    /// bins. It should be assumed that none of the data at the `block` output location
    /// is valid after this operation is performed.
    /// This is the only method that RawPool uses to re-use freed blocks
    pub unsafe fn pop(&mut self, pool: &mut RawPool, blocks: block) -> Option<block>{
        assert!(blocks != 0);
        if self.len == 0 {
            return None;
        }
        // get the starting bin
        // the starting bin is where we KNOW we can find the required amount of
        // data (if it has any) and is the fastest way to retrive data from a bin.
        let bin: u8 = match blocks {
            1            => 0,
            2   ...4     => 1,
            5   ...16    => 2,
            17  ...64    => 3,
            65  ...256   => 4,
            257 ...1024  => 5,
            _            => 6,
        };
        let poolptr = pool as *mut RawPool;
        for b in bin..(NUM_BINS - 1) {
            let poolptr = pool as *mut RawPool;
            if let Some(out) = self.bins[b as usize].root_mut(&mut *poolptr) {
                // we have found freed data that is >= the size we need
                self.consume_partof(pool, out, blocks);
                return Some(out.block());
            }
        }

        // failed to get in any of the smaller bins (or the data is too large)
        // have to search item by item in the final (largest) bin.
        if let Some(mut out) = self.bins[(NUM_BINS - 1) as usize].root_mut(&mut *poolptr) {
            loop {
                if out.blocks() >= blocks {
                    // we have found freed data that is >= the size we need
                    self.consume_partof(pool, out, blocks);
                    return Some(out.block());
                }
                let out = match out.next() {
                    Some(o) => o,
                    None => return None,
                };
            }
        }
        None
    }

    /// Consume partof the freed value, reducing it's actual size to the size of `blocks`.
    /// This splits the freed value (or just removes if it is the exact requested size)
    /// and removes it from being tracked by the Bins. It then tracks whatever
    /// was left of it.
    ///
    /// After perforing this operation, the information stored in `freed` is completely invalid.
    /// This includes it's blocks-size, block-location as well as `prev` and `next` fields. It
    /// is the responsibility of the user to set the information to be valid.
    unsafe fn consume_partof(&mut self, pool: &mut RawPool, freed: &mut Free, blocks: block) {
        // all unsafe operations are safe because we know that we are
        // never changing more than one freed block at a time
        let old_blocks = freed.blocks();
        if old_blocks == blocks {
            // perfectly equal, consumes freed block
            let old_block = freed.block();
            freed.remove(pool);
        } else {
            // use only the size that is needed, so append a new freed block
            let old_block = freed.block();
            let new_block = old_block + blocks;
            let new_freed = pool.freed_mut(new_block)
                as *mut Free;
            (*new_freed) = Free {
                _blocks: old_blocks - blocks,
                _block: new_block,
                _prev: BLOCK_NULL,
                _next: BLOCK_NULL,
            };
            freed.remove(pool); // has to come before insert
            self.insert(pool, &mut *new_freed);
        }
    }
}
