#[cfg(test)] use std::vec::Vec;
#[cfg(test)] use std::iter::FromIterator;
#[cfg(test)] use core::mem;

use core::default::Default;
use core::fmt;

use super::types::*;
use super::raw_pool::*;

// ##################################################
// # Free

/// the Free struct is a linked list of free values with
/// the root as a size-bin in pool
#[repr(C, packed)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct Free {
    // NOTE: DO NOT MOVE `_blocks`, IT IS SWAPPED WITH `Full._blocks`
    // The first bit of `_blocks` is always 0 for Free structs
    pub _blocks: BlockLoc,        // size of this freed memory
    pub _block: BlockLoc,          // block location of this struct
    pub _prev: BlockLoc,          // location of previous freed memory
    pub _next: BlockLoc,          // location of next freed memory
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
        write!(f, "Free{}{{blocks: {}, block: {}, prev: {}, next: {}}}{}",
               isvalid,
               self._blocks & BLOCK_BITMAP,
               self._block & BLOCK_BITMAP,
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
    pub fn block(&self) -> BlockLoc {
        self.assert_valid();
        self._block
    }

    /// blocks accessor, handling any bitmaps
    pub fn blocks(&self) -> BlockLoc {
        self.assert_valid();
        self._blocks
    }

    /// prev accessor, handling any bitmaps
    pub fn prev(&self) -> Option<BlockLoc> {
        self.assert_valid();
        if self._prev == BLOCK_NULL {
            None
        } else {
            Some(self._prev)
        }
    }

    /// next accessor, handling any bitmaps
    pub fn next(&self) -> Option<BlockLoc> {
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
                self._prev = BLOCK_NULL;
                let pool = pool as *mut RawPool;
                let bin = (*pool).freed_bins.get_insert_bin(self.blocks());
                (*pool).freed_bins.bins[bin as usize]._root = self.block();
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

    // /// append a freed block after this one
    // unsafe fn append(&mut self, pool: &mut RawPool, next: &mut Free) {
    //     self.assert_valid();
    //     let pool = pool as *mut RawPool;
    //     if let Some(n) = self.next() {
    //         // set prev of the next freed block (if it exists)
    //         (*pool).freed_mut(n).set_prev(&mut (*pool), Some(next));
    //     }
    //     self.set_next(Some(next));
    // }

    /// remove self from the freed pool
    /// this also keeps track of the statistics for number of freed blocks
    pub unsafe fn remove(&mut self, pool: &mut RawPool) {
        self.assert_valid();
        /// convinience function for this method only
        unsafe fn get_freed(pool: &mut RawPool, block: Option<BlockLoc>) -> Option<&mut Free> {
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
                let bin = pool.freed_bins.get_insert_bin(self.blocks());
                match get_freed(&mut *poolp, self.next()) {
                    Some(next) => {
                        next.set_prev(&mut *poolp, None);
                        assert_eq!(pool.freed_bins.bins[bin as usize]._root, next.block());
                    },
                    None => {
                        pool.freed_bins.bins[bin as usize]._root = BLOCK_NULL;
                    }
                }
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

#[test]
/// free has the capability of causing a lot of bugs if done
/// incorrectly. It's functionality must be completely
/// tested.
fn test_free() {
    unsafe {
        let (mut indexes, mut blocks): ([Index; 256], [Block; 4096]) = (
            [Index::default(); 256], mem::zeroed());
        let iptr: *mut Index = mem::transmute(&mut indexes[..][0]);
        let bptr: *mut Block = mem::transmute(&mut blocks[..][0]);

        let mut pool = RawPool::new(iptr, indexes.len() as IndexLoc, bptr, blocks.len() as BlockLoc);
        let p = &mut pool as *mut RawPool;

        // ok, we are completely ignoring bins for this, set everything
        // up manaually
        let f1 = pool.freed_mut(0);
        let f2 = pool.freed_mut(10);
        let f3 = pool.freed_mut(20);
        let f4 = pool.freed_mut(25);

        f1._block = 0;
        f1._blocks = 10;
        assert_eq!(f1.block(), 0);
        assert_eq!(f1.blocks(), 10);

        f2._block = 10;
        f2._blocks = 10;
        assert_eq!(f2.block(), 10);
        assert_eq!(f2.blocks(), 10);

        f3._block = 20;
        f3._blocks = 5;
        assert_eq!(f3.block(), 20);
        assert_eq!(f3.blocks(), 5);

        f4._block = 25;
        f4._blocks = 2;
        assert_eq!(f4.block(), 25);
        assert_eq!(f4.blocks(), 2);

        (*p).heap_block = 27;
        (*p).freed_bins.len = 4;

        // set things one by one and check them
        // f3 -> f1 -> f2 -> f4
        f3._prev = BLOCK_NULL;
        f3.set_next(Some(f1));
        assert_eq!(f3.prev(), None);
        assert_eq!(f3.next(), Some(f1.block()));
        assert_eq!(f1.prev(), Some(f3.block()));

        f1.set_next(Some(f2));
        assert_eq!(f1.next(), Some(f2.block()));
        assert_eq!(f2.prev(), Some(f1.block()));

        f2.set_next(Some(f4));
        assert_eq!(f2.next(), Some(f4.block()));
        assert_eq!(f4.prev(), Some(f2.block()));

        f4.set_next(None);
        assert_eq!(f4.next(), None);

        // just double check that these didn't change...
        assert_eq!(f3.prev(), None);
        assert_eq!(f3.next(), Some(f1.block()));
        assert_eq!(f1.prev(), Some(f3.block()));
        assert_eq!(f1.next(), Some(f2.block()));
        assert_eq!(f2.prev(), Some(f1.block()));
        assert_eq!(f2.next(), Some(f4.block()));
        assert_eq!(f4.prev(), Some(f2.block()));

        // test remove... without using bins (so only middle)
        f1.remove(&mut *p);
        assert_eq!((*p).freed_bins.len, 3);
        assert_eq!(f3.next(), Some(f2.block()));
        assert_eq!(f2.prev(), Some(f3.block()));

        f2.remove(&mut *p);
        assert_eq!((*p).freed_bins.len, 2);
        assert_eq!(f3.next(), Some(f4.block()));
        assert_eq!(f4.prev(), Some(f3.block()));

        f4.remove(&mut *p);
        assert_eq!((*p).freed_bins.len, 1);
        assert_eq!(f3.next(), None);
        assert_eq!(f3.prev(), None);
    }
}

// ##################################################
// # Freed Bins and Root

/// `FreedRoot' stores the beginning of the linked list
/// and keeps track of statistics
#[repr(C, packed)]
pub struct FreedRoot {
    pub _root: BlockLoc,
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

/// `FreedBins` provide simple and fast access to freed data
#[derive(Default)]
pub struct FreedBins {
    pub len: BlockLoc,
    pub bins: [FreedRoot; NUM_BINS as usize],
}

impl FreedBins {
    /// get the bin that would be used when
    /// inserting a Free value
    pub fn get_insert_bin(&self, blocks: BlockLoc) -> u8 {
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
        assert!(freed.block() + freed.blocks() <= pool.heap_block, "{:?}", freed);
        self.len += 1;
        let bin = self.get_insert_bin(freed.blocks());
        self.bins[bin as usize].insert_root(pool, freed);
    }

    /// get a block of the requested size from the freed bins, removing it from the freed
    /// bins. It should be assumed that none of the data at the `block` output location
    /// is valid after this operation is performed.
    /// This is the only method that RawPool uses to re-use freed blocks
    pub unsafe fn pop(&mut self, pool: &mut RawPool, blocks: BlockLoc) -> Option<BlockLoc>{
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
                out = match out.next() {
                    Some(o) => (*poolptr).freed_mut(o),
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
    /// After performing this operation, the information stored in `freed` is completely invalid.
    /// This includes it's blocks-size, block-location as well as `prev` and `next` fields. It
    /// is the responsibility of the user to set the information to be valid.
    unsafe fn consume_partof(&mut self, pool: &mut RawPool, freed: &mut Free, blocks: BlockLoc) {
        // all unsafe operations are safe because we know that we are
        // never changing more than one freed block at a time
        let old_blocks = freed.blocks();
        if old_blocks == blocks {
            // perfectly equal, consumes freed block
            freed.remove(pool);
        } else {
            // use only the size that is needed, so append a new freed block
            assert!(old_blocks > blocks);
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


#[test]
fn test_bins() {
    unsafe {
        let (mut indexes, mut blocks): ([Index; 256], [Block; 4096]) = (
            [Index::default(); 256], mem::zeroed());
        let iptr: *mut Index = mem::transmute(&mut indexes[..][0]);
        let bptr: *mut Block = mem::transmute(&mut blocks[..][0]);

        let mut pool = RawPool::new(iptr, indexes.len() as IndexLoc, bptr, blocks.len() as BlockLoc);
        let p = &mut pool as *mut RawPool;

        // allocate and free through normal process
        let bin1 = Vec::from_iter(
            (0..5).map(|_| pool.alloc_index(10).unwrap()));
        let bin1_blocks: Vec<_> = bin1.iter().map(|i| pool.index(*i).block()).collect();

        let bin2 = Vec::from_iter(
            (0..5).map(|_| pool.alloc_index(20).unwrap()));
        let f1_i = pool.alloc_index(1).unwrap();
        let f1_index = (*p).index(f1_i);
        assert_eq!(f1_index.block(), 150);
        assert_eq!(f1_i, 10);

        for (i1, i2) in bin1.iter().zip(bin2.iter()) {
            pool.dealloc_index(*i2);
            pool.dealloc_index(*i1);
        }

        assert_eq!(pool.freed_bins.len, 10);

        // go through bin1, asserting that it makes sense
        let bin1_freed: Vec<_> = bin1_blocks.iter().map(|b| (*p).freed_mut(*b)).collect();
        let b1_0 = pool.freed_mut(0);
        let b1_1 = pool.freed_mut(10);
        let b1_2 = pool.freed_mut(20);
        let b1_3 = pool.freed_mut(30);
        let b1_4 = pool.freed_mut(40);

        let b2_0 = pool.freed_mut(50);
        let b2_1 = pool.freed_mut(70);
        let b2_2 = pool.freed_mut(90);
        let b2_3 = pool.freed_mut(110);
        let b2_4 = pool.freed_mut(130);

        // bins are a first in / last out buffer
        assert_eq!(pool.freed_bins.bins[1]._root, b1_4.block());
        assert_eq!(b1_4.prev(), None);
        assert_eq!(b1_4.next(), Some(b1_3.block()));

        assert_eq!(b1_3.prev(), Some(b1_4.block()));
        assert_eq!(b1_3.next(), Some(b1_2.block()));

        assert_eq!(b1_2.prev(), Some(b1_3.block()));
        assert_eq!(b1_2.next(), Some(b1_1.block()));

        assert_eq!(b1_1.prev(), Some(b1_2.block()));
        assert_eq!(b1_1.next(), Some(b1_0.block()));

        assert_eq!(b1_0.prev(), Some(b1_1.block()));
        assert_eq!(b1_0.next(), None);

        // test remove
        b1_4.remove(&mut *p);
        assert_eq!(pool.freed_bins.len, 9);
        assert_eq!(pool.freed_bins.bins[1]._root, b1_3.block());
        assert_eq!(b1_3.prev(), None);
        assert_eq!(b1_3.next(), Some(b1_2.block()));

        // re-insert, everything should go back to same as before
        (*p).freed_bins.insert(&mut *p, b1_4);
        assert_eq!(pool.freed_bins.len, 10);
        assert_eq!(pool.freed_bins.bins[1]._root, b1_4.block());
        assert_eq!(b1_4.prev(), None);
        assert_eq!(b1_4.next(), Some(b1_3.block()));
        assert_eq!(b1_3.prev(), Some(b1_4.block()));
        assert_eq!(b1_3.next(), Some(b1_2.block()));

        // test "simpler" join
        b1_0.join(&mut *p, b1_1);
        let b2_5 = pool.freed_mut(b1_0.block());
        assert_eq!(pool.freed_bins.len, 9);
        assert_eq!(b2_5.blocks(), 20);
        assert_eq!(pool.freed_bins.bins[2]._root, b2_5.block());
        assert_eq!(b2_5.prev(), None);
        assert_eq!(b2_5.next(), Some(b2_4.block()));

        // more complex join... joining things from separate bins
        // not actually that much more complex in the logic though
        b2_5.join(&mut *p, b1_2);
        assert_eq!(pool.freed_bins.len, 8);
        assert_eq!(b2_5.blocks(), 30);
        assert_eq!(pool.freed_bins.bins[2]._root, b2_5.block());
        assert_eq!(b2_5.prev(), None);
        assert_eq!(b2_5.next(), Some(b2_4.block()));

        b2_5.join(&mut *p, b1_3);
        b2_5.join(&mut *p, b1_4);
        assert_eq!(pool.freed_bins.len, 6);
        assert_eq!(b2_5.blocks(), 50);
        assert_eq!(pool.freed_bins.bins[2]._root, b2_5.block());
        assert_eq!(b2_5.prev(), None);
        assert_eq!(b2_5.next(), Some(b2_4.block()));

        b2_5.join(&mut *p, b2_0);
        let b3_0 = pool.freed_mut(b2_5.block());
        assert_eq!(pool.freed_bins.len, 5);
        assert_eq!(b2_5.blocks(), 70);
        assert_eq!(pool.freed_bins.bins[2]._root, b2_4.block());
        assert_eq!(pool.freed_bins.bins[3]._root, b3_0.block());
        assert_eq!(b3_0.prev(), None);
        assert_eq!(b3_0.next(), None);

        assert_eq!(b2_4.prev(), None);
        assert_eq!(b2_4.next(), Some(b2_3.block()));

        // this is just a really good place to test clean and defrag too...
        (*p).clean();
        assert_eq!(pool.freed_bins.len, 1);
        assert_eq!(b1_0.blocks(), 150);

        (*p).defrag();
        assert_eq!(pool.freed_bins.len, 0);
        assert_eq!(pool.heap_block, 1);
        let f1_0 = pool.full(0);
        f1_0.assert_valid();
        assert_eq!(f1_0.blocks(), 1);
        assert_eq!(f1_0.index(), f1_i);
        assert_eq!(f1_index.block(), 0);
    }
}

