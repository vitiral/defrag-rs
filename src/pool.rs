/*! pool.rs
Contains all the logic related to the RawPool. The RawPool is
what actually contains and maintains the indexes and memory.
*/


use core;
use core::mem;
use core::default::Default;
use core::slice;

use super::types::*;

// ##################################################
// # Block

// a single block, currently == 2 Free blocks which
// is 128 bits
#[repr(packed)]
#[derive(Copy, Clone)]
pub struct Block {
    _data: Free,
    _a: Free,
}

impl Default for Block {
    fn default() -> Block {
        unsafe { mem::zeroed() }
    }
}

// ##################################################
// # Index

/// Index provides a non-moving location in memory for userspace
/// to reference. There is a limited number of indexes.
#[repr(packed)]
#[derive(Debug, Copy, Clone)]
pub struct Index {
    _block: block,
}

impl Default for Index {
    fn default() -> Index {
        Index {_block: BLOCK_NULL}
    }
}

impl Index {
    /// get the block where the index is stored
    pub fn block(&self) -> block {
        self._block
    }

    /// get size of Index DATA in bytes
    pub fn size(&self, pool: &RawPool) -> usize {
        unsafe {
            pool.full(self.block()).blocks() as usize * mem::size_of::<Block>()
                - mem::size_of::<Full>()
        }
    }
}

// ##################################################
// # Free

/// the Free struct is a linked list of free values with
/// the root as a size-bin in pool
#[repr(packed)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Free {
    // NOTE: DO NOT MOVE `_blocks`, IT IS SWAPPED WITH `_blocks` IN `Full`
    // The first bit of `_blocks` is always 0 for Free structs
    _blocks: block,        // size of this freed memory
    _block: block,          // block location of this struct
    _prev: block,          // location of previous freed memory
    _next: block,          // location of next freed memory
    // data after this (until block + blocks) is invalid
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

impl Free {
    /// block accessor
    fn block(&self) -> block {
        self._block
    }

    /// blocks accessor, handling any bitmaps
    fn blocks(&self) -> block {
        self._blocks
    }

    /// prev accessor, handling any bitmaps
    fn prev(&self) -> Option<block> {
        if self._prev == BLOCK_NULL {
            None
        } else {
            Some(self._prev)
        }
    }

    /// next accessor, handling any bitmaps
    fn next(&self) -> Option<block> {
        if self._next == BLOCK_NULL {
            None
        } else {
            Some(self._next)
        }
    }

    unsafe fn set_next(&mut self, next: Option<&mut Free>) {
        match next {
            Some(n) => {
                self._next = n.block();
                n._prev = self.block();
            }
            None => self._next = BLOCK_NULL,
        }
    }

    /// set the prev freed block and set it's next to self
    unsafe fn set_prev(&mut self, pool: &mut RawPool, prev: Option<&mut Free>) {
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

    /// append a freed block after this one
    pub unsafe fn append(&mut self, pool: &mut RawPool, next: &mut Free) {
        let pool = pool as *mut RawPool;
        if let Some(n) = self.next() {
            // set prev of the next freed block (if it exists)
            (*pool).freed_mut(n).set_prev(&mut (*pool), Some(next));
        }
        self.set_next(Some(next));
    }

    /// remove self from the freed pool
    /// this also keeps track of the statistics for number of freed blocks
    pub unsafe fn remove(&mut self, pool: &mut RawPool) {
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
}

// ##################################################
// # Full

/// the Full struct represents data that has been allocated
/// it contains only the size and a back-reference to the index
/// that references it.
/// It also contains the lock information inside it's index.
#[repr(packed)]
#[derive(Debug)]
pub struct Full {
    // NOTE: DO NOT MOVE `_blocks`, IT IS SWAPPED WITH `_blocks` IN `Free`
    // The first bit of blocks is 1 for Full structs
    _blocks: block,        // size of this freed memory
    _index: index,         // data which contains the index and the lock information
    // space after this (until block + blocks) is invalid
}

impl Full {
    /// get whether Full is locked
    pub fn is_locked(&self) -> bool {
        self._index & INDEX_HIGH_BIT == INDEX_HIGH_BIT
    }

    /// set the lock on Full
    pub fn set_lock(&mut self) {
        self._index |= INDEX_HIGH_BIT;
    }

    /// clear the lock on Full
    pub fn clear_lock(&mut self) {
        self._index &= INDEX_BITMAP
    }

    fn blocks(&self) -> block {
        self._blocks & BLOCK_BITMAP
    }

    fn index(&self) -> block {
        self._index & INDEX_BITMAP
    }
}

// ##################################################
// # Freed Bins and Root

/// a FreedRoot stores the beginning of the linked list
/// and keeps track of statistics
#[repr(packed)]
struct FreedRoot {
    _root: block,
}

impl Default for FreedRoot {
    fn default() -> FreedRoot {
        FreedRoot {_root: BLOCK_NULL}
    }
}

impl FreedRoot {
    unsafe fn root<'a>(&self, pool: &'a RawPool) -> Option<&'a Free> {
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
        if let Some(cur_root) = self.root_mut(&mut *(pool as *mut RawPool)) {
            cur_root.set_prev(pool, Some(freed));
        }
        freed._prev = BLOCK_NULL;  // TODO: this is probably not necessary
        self._root = freed.block();
    }
}

const NUM_BINS: u8 = 7;

/// the FreedBins provide simple and fast access to freed data
#[derive(Default)]
struct FreedBins {
    len: block,
    bins: [FreedRoot; NUM_BINS as usize],
}

impl FreedBins {
    /// get the bin that would be used when
    /// inserting a Free value
    fn get_insert_bin(&self, blocks: block) -> u8 {
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

    /// insert a Free block into a freed bin
    unsafe fn insert(&mut self, pool: &mut RawPool, freed: &mut Free) {
        self.len += 1;
        let bin = self.get_insert_bin(freed.blocks());
        self.bins[bin as usize].insert_root(pool, freed);
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

    /// get a block of the requested size from the freed bins, removing it from the freed
    /// bins. It should be assumed that none of the data at the `block` output location
    /// is valid after this operation is performed.
    unsafe fn pop(&mut self, pool: &mut RawPool, blocks: block) -> Option<block>{
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
}

// ##################################################
// # RawPool

/// The RawPool is the private container and manager for all allocated
/// and freed data. It handles the internals of allocation, deallocation
/// and defragmentation.
pub struct RawPool {
    // blocks and statistics
    _blocks: *mut Block,     // actual data
    _blocks_len: block,      // len of blocks
    heap_block: block,       // the current location of the "heap"
    blocks_used: block,       // total memory currently used

    // indexes and statistics
    _indexes: *mut Index,    // does not move and stores movable block location of data
    _indexes_len: index,     // len of indexes
    last_index_used: index,  // for speeding up finding indexes
    indexes_used: index,     // total number of indexes used

    /// freed bins
    freed_bins: FreedBins,           // freed bins

}

impl RawPool {
    /// get a new RawPool
    ///
    /// This operation is unsafe because it is up to the user to ensure
    /// that `indexes` and `blocks` do not get deallocated for the lifetime
    /// of RawPool
    pub unsafe fn new(indexes: *mut Index, indexes_len: index,
               blocks: *mut Block, blocks_len: block)
               -> RawPool {
        if indexes_len > index::max_value() / 2 {
            panic!("indexes_len too large");
        }
        if blocks_len > block::max_value() / 2 {
            panic!("blocks_len too large");
        }
        // initialize all indexes to INDEX_NULL
        let mut ptr = indexes;
        for _ in 0..indexes_len {
            *ptr = Index::default();
            ptr = ptr.offset(1);
        }

        // initialize all blocks to 0
        let mut ptr = blocks;
        for _ in 0..blocks_len {
            *ptr = Block::default();
            ptr = ptr.offset(1);
        }

        RawPool {
            _blocks: blocks,
            _blocks_len: blocks_len,
            heap_block: 0,
            blocks_used: 0,

            _indexes: indexes,
            _indexes_len: indexes_len,
            last_index_used: indexes_len - 1,
            indexes_used: 0,

            freed_bins: FreedBins::default(),
        }
    }

    // public safe API

    /// raw pool size in blocks
    pub fn len_blocks(&self) -> block {
        self._blocks_len
    }

    pub fn len_indexes(&self) -> block {
        self._indexes_len
    }

    /// raw pool size in bytes
    pub fn size(&self) -> usize {
        self.len_blocks() as usize * mem::size_of::<Block>()
    }

    /// raw blocks remaining (fragmented or not)
    pub fn blocks_remaining(&self) -> block {
        self.len_blocks() - self.blocks_used
    }

    /// raw bytes remaining (fragmented or not)
    pub fn bytes_remaining(&self) -> usize {
        self.blocks_remaining() as usize * mem::size_of::<Block>()
    }

    pub fn indexes_remaining(&self) -> index {
        self.len_indexes() - self.indexes_used
    }

    /// get the index
    pub unsafe fn index(&self, i: index) -> &Index {
        let ptr = self._indexes.offset(i as isize);
        mem::transmute(ptr)
    }

    // public unsafe API

    pub unsafe fn index_mut(&self, i: index) -> &mut Index {
        let ptr = self._indexes.offset(i as isize);
        mem::transmute(ptr)
    }

    /// get the raw ptr to the data at block
    pub unsafe fn ptr(&self, block: block) -> *const u8 {
        let free = self.freed(block);
        let mut ptr: *const u8 = mem::transmute(free);
        ptr = ptr.offset(mem::size_of::<Full>() as isize);
        ptr
    }

    /// read the block as a Free block
    pub unsafe fn freed(&self, block: block) -> &Free {
        let ptr = self._blocks.offset(block as isize);
        mem::transmute(ptr)
    }

    /// mut the block as a Free block
    pub unsafe fn freed_mut(&self, block: block) -> &mut Free {
        let ptr = self._blocks.offset(block as isize);
        mem::transmute(ptr)
    }

    /// read the block as a Full block
    pub unsafe fn full(&self, block: block) -> &Full {
        let ptr = self._blocks.offset(block as isize);
        mem::transmute(ptr)
    }

    /// mut the block as a Full block
    pub unsafe fn full_mut(&self, block: block) -> &mut Full {
        let ptr = self._blocks.offset(block as isize);
        mem::transmute(ptr)
    }

    /// get an unused index
    fn get_unused_index(&mut self) -> Result<index> {
        // TODO: this is currently pretty slow, maybe cache freed indexes?
        let mut i = (self.last_index_used + 1) % self.len_indexes();
        while i != self.last_index_used {
            if unsafe{self.index(i)._block} == BLOCK_NULL {
                self.last_index_used = i;
                return Ok(i);
            }
            i = (i + 1) % self.len_indexes();
        }
        return Err(Error::OutOfIndexes);
    }

    unsafe fn use_freed(&mut self, blocks: block) -> Option<block> {
        let selfptr = self as *mut RawPool;
        match (*selfptr).freed_bins.pop(self, blocks) {
            Some(f) => Some(f),
            None => None,
        }
    }

    /// allocate data with a specified number of blocks,
    /// including the half block required to store `Full` struct
    pub unsafe fn alloc_index(&mut self, blocks: block) -> Result<index> {
        if blocks == 0 {
            return Err(Error::InvalidSize);
        }
        if self.blocks_remaining() < blocks {
            return Err(Error::OutOfMemory);
        }
        let prev_used_index = self.last_index_used;
        let i = try!(self.get_unused_index());
        let block = if let Some(block) = self.use_freed(blocks) {
            // we are reusing previously freed data
            block
        } else if (self.heap_block + blocks) <= self.len_blocks() {
            // we are using data on the heap
            let block = self.heap_block;
            self.heap_block += blocks;
            block
        } else {
            // we couldn't find enough contiguous memory (even though it exists)
            // i.e. we are too fragmented
            self.last_index_used = prev_used_index;
            return Err(Error::Fragmented)
        };
        self.indexes_used += 1;
        // set the index data
        unsafe {
            let index = self.index_mut(i);
            index._block = block
        }
        // set the full data in the block
        unsafe {
            let full = self.full_mut(block);
            full._blocks = blocks | BLOCK_HIGH_BIT;
            full._index = i;  // starts unlocked
        }
        Ok(i)
    }

    /// dealoc an index from the pool, this WILL corrupt any data that was there
    pub unsafe fn dealloc_index(&mut self, i: index) {
        self.indexes_used -= 1;

        // get the size and location from the Index and clear it
        let block = self.index(i).block();
        *self.index_mut(i) = Index::default();
        // set up freed
        let freed = self.freed_mut(block) as *mut Free;
        (*freed)._blocks &= BLOCK_BITMAP;  // set first bit to 0
        (*freed)._block = block;
        (*freed)._next = BLOCK_NULL;

        let selfptr = self as *mut RawPool;
        (*selfptr).freed_bins.insert(self, &mut *freed);
    }
}

// ##################################################
// # Internal Tests

#[test]
fn test_basic() {
    // assert that our bitmap is as expected
    let highbit = 1 << ((mem::size_of::<block>() * 8) - 1);
    assert_eq!(BLOCK_BITMAP, !highbit, "{:b} != {:b}", BLOCK_BITMAP, !highbit);
    assert_eq!(BLOCK_HIGH_BIT, highbit, "{:b} != {:b}", BLOCK_HIGH_BIT, highbit);

    // assert that Full.blocks() cancels out the high bit
    let expected = highbit ^ block::max_value();
    let mut f = Full {
        _blocks: BLOCK_NULL,
        _index: INDEX_NULL,
    };
    assert_eq!(f.blocks(), expected);
    f._blocks = highbit;
    assert_eq!(f.blocks(), 0);
    f._blocks = 0;
    assert_eq!(f.blocks(), 0);

    // assert that Free.blocks() DOESN'T cancel out the high bit (not necessary)
    let mut f = Free {
        _blocks: block::max_value(),
        _block: 0,
        _prev: 0,
        _next: 0,
    };
    assert_eq!(f.blocks(), block::max_value());
    f._blocks = highbit;
    assert_eq!(f.blocks(), highbit);
    f._blocks = 0;
    assert_eq!(f.blocks(), 0);
}

#[test]
/// test using raw indexes
fn test_indexes() {
    unsafe {
    let mut indexes = [Index::default(); 256];
    let mut blocks = [Block::default(); 4096];
    let iptr: *mut Index = mem::transmute(&mut indexes[..][0]);
    let bptr: *mut Block = mem::transmute(&mut blocks[..][0]);

    let mut pool = RawPool::new(iptr, indexes.len() as index, bptr, blocks.len() as block);
    assert_eq!(pool.get_unused_index().unwrap(), 0);
    assert_eq!(pool.get_unused_index().unwrap(), 1);
    assert_eq!(pool.get_unused_index().unwrap(), 2);

    // allocate an index
    println!("allocate 1");
    assert_eq!(pool.freed_bins.len, 0);
    let i = pool.alloc_index(4).unwrap();
    assert_eq!(i, 3);
    let block;
    {
        let index = (*(&pool as *const RawPool)).index(i);
        assert_eq!(index._block, 0);
        block = index.block();
        assert_eq!(pool.full(index.block()).blocks(), 4);
        assert_eq!(pool.full(index.block())._blocks, BLOCK_HIGH_BIT | 4);
    }

    // deallocate the index
    println!("free 1");
    assert_eq!(pool.freed_bins.len, 0);
    pool.dealloc_index(i);
    {
        println!("deallocated 1");
        assert_eq!(pool.index(i)._block, BLOCK_NULL);

        let freed = pool.freed(block);
        assert_eq!(freed.blocks(), 4);
        assert_eq!(freed._blocks, 4);
        assert_eq!(freed.prev(), None);
        assert_eq!(freed.next(), None);
        let bin = pool.freed_bins.get_insert_bin(4);
        assert_eq!(pool.freed_bins.bins[bin as usize].root(&pool).unwrap().block(), block);
    }

    // allocate another index (that doesn't fit in the first)
    println!("allocate 2");
    let i2 = pool.alloc_index(8).unwrap();
    assert_eq!(i2, 4);
    let block2;
    {
        let index2 = (*(&pool as *const RawPool)).index(i2);
        assert_eq!(index2._block, 4);
        block2 = index2.block();
        assert_eq!(pool.full(index2.block()).blocks(), 8);
        assert_eq!(pool.full(index2.block())._blocks, BLOCK_HIGH_BIT | 8);
    }

    // allocate a 3rd index (that does fit in the first)
    println!("allocate 3");
    let i3 = pool.alloc_index(2).unwrap();
    assert_eq!(i3, 5);
    let block3;
    {
        let index3 = (*(&pool as *const RawPool)).index(i3);
        assert_eq!(index3._block, 0);
        block3 = index3.block();
        assert_eq!(pool.full(index3.block()).blocks(), 2);
        assert_eq!(pool.full(index3.block())._blocks, BLOCK_HIGH_BIT | 2);
    }
    // tests related to the fact that i3 just overwrote the freed item
    let bin = pool.freed_bins.get_insert_bin(2);
    let free_block = pool.freed_bins.bins[bin as usize].root(&pool).unwrap().block();
    assert_eq!(free_block, 2);
    {
        // free1 has moved
        let free = pool.freed(free_block);
        assert_eq!(free.blocks(), 2);
        assert_eq!(free.prev(), None);
        assert_eq!(free.next(), None);
    }
    }
}
