/*
Contains all the logic related to the RawPool. The RawPool is
what actually contains and maintains the indexes and memory.
*/

use core::mem;
use core::ptr;
use core::slice;
use core::default::Default;
use core::fmt;

use super::types::*;
use super::free::{FreedBins, Free, NUM_BINS};
use super::utils;

// ##################################################
// # Block

// a single block, currently == 2 Free blocks which
// is 128 bits

#[derive(Debug, Eq, PartialEq)]
pub enum BlockType {
    Free,
    Full,
}

#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct Block {
    _a: Free,
    _b: Free,
}

impl fmt::Debug for Block {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "B<{:?}> {{blocks: {}}}",
               self.ty(), self._a._blocks & BLOCK_BITMAP)
    }
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        unsafe {
            match self.ty() {
                BlockType::Free => write!(f, "{:?}", self.as_free()),
                BlockType::Full => write!(f, "{:?}", self.as_full()),
            }
        }
    }
}

impl Block {
    pub fn blocks(&self) -> block {
        let out = self._a._blocks & BLOCK_BITMAP;
        assert!(out != 0);
        out
    }

    pub unsafe fn block(&self, pool: &RawPool) -> block {
        match self.ty() {
            BlockType::Free => self.as_free().block(),
            BlockType::Full => pool.index(self.as_full().index()).block(),
        }
    }

    pub unsafe fn next_mut(&mut self, pool: &RawPool) -> Option<&mut Block> {
        let block = self.block(pool);
        let blocks = self.blocks();
        if block + blocks == pool.heap_block {
            None
        } else {
            assert!(block + blocks < pool.heap_block);
            let ptr = self as *mut Block;
            assert!((*ptr).blocks() != 0);
            Some(mem::transmute(ptr.offset(blocks as isize)))
        }
    }

    pub fn ty(&self) -> BlockType {
        if self._a._blocks & BLOCK_HIGH_BIT == 0 {
            BlockType::Free
        } else {
            BlockType::Full
        }
    }

    pub unsafe fn as_free(&self) -> &Free {
        assert_eq!(self.ty(), BlockType::Free);
        let ptr = self as *const Block;
        mem::transmute(ptr)
    }

    pub unsafe fn as_free_mut(&mut self) -> &mut Free {
        assert_eq!(self.ty(), BlockType::Free);
        let ptr = self as *mut Block;
        mem::transmute(ptr)
    }

    pub unsafe fn as_full(&self) -> &Full {
        assert_eq!(self.ty(), BlockType::Full);
        let ptr = self as *const Block;
        mem::transmute(ptr)
    }

    pub unsafe fn as_full_mut(&mut self) -> &mut Full {
        assert_eq!(self.ty(), BlockType::Full);
        let ptr = self as *mut Block;
        mem::transmute(ptr)
    }
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
#[repr(C, packed)]
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

    pub fn block_maybe(&self) -> Option<block> {
        if self._block == BLOCK_NULL {
            None
        } else {
            Some(self._block)
        }
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
// # Full

/// the Full struct represents data that has been allocated
/// it contains only the size and a back-reference to the index
/// that references it.
/// It also contains the lock information inside it's index.
#[repr(C, packed)]
#[derive(Copy, Clone)]
pub struct Full {
    // NOTE: DO NOT MOVE `_blocks`, IT IS SWAPPED WITH `_blocks` IN `Free`
    // The first bit of blocks is 1 for Full structs
    _blocks: block,        // size of this freed memory
    _index: index,         // data which contains the index and the lock information
    // space after this (until block + blocks) is invalid
}

impl fmt::Debug for Full {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let isvalid = if self.is_valid() {" "} else {"!"};
        write!(f, "Full{}{{blocks: {}, index: {}, locked: {}}}{}",
               isvalid,
               self._blocks & BLOCK_BITMAP,
               self._index & BLOCK_BITMAP,
               if self._index & INDEX_HIGH_BIT == INDEX_HIGH_BIT {1} else {0},
               isvalid
        )
    }
}

impl Full {
    /// get whether Full is locked
    pub fn is_locked(&self) -> bool {
        self.assert_valid();
        self._index & INDEX_HIGH_BIT == INDEX_HIGH_BIT
    }

    /// set the lock on Full
    pub fn set_lock(&mut self) {
        self.assert_valid();
        self._index |= INDEX_HIGH_BIT;
    }

    /// clear the lock on Full
    pub fn clear_lock(&mut self) {
        self.assert_valid();
        self._index &= INDEX_BITMAP
    }

    fn blocks(&self) -> block {
        self.assert_valid();
        self._blocks & BLOCK_BITMAP
    }

    fn index(&self) -> block {
        self.assert_valid();
        self._index & INDEX_BITMAP
    }

    fn is_valid(&self) -> bool {
        self._blocks & BLOCK_HIGH_BIT == BLOCK_HIGH_BIT && self._blocks & BLOCK_BITMAP != 0
    }

    fn assert_valid(&self) {
        assert!(self.is_valid(), "{:?}", self);
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
    pub heap_block: block,       // the current location of the "heap"
    pub blocks_used: block,       // total memory currently used

    // indexes and statistics
    _indexes: *mut Index,    // does not move and stores movable block location of data
    _indexes_len: index,     // len of indexes
    last_index_used: index,  // for speeding up finding indexes
    indexes_used: index,     // total number of indexes used

    // freed data
    pub freed_bins: FreedBins,           // freed bins

}

pub struct DisplayRawPool<'a> {
    pool: &'a RawPool,
}

impl<'a> fmt::Display for DisplayRawPool<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let p = self.pool;
        let mut o;
        o = write!(f, "RawPool {{\n");
        write!(f, "  * index_ptr: {:?}\n", p._indexes);
        write!(f, "  * Indexes:  len: {}, used: {} remaining: {}\n",
               p.len_indexes(), p.indexes_used, p.indexes_remaining());
        for i in 0..p.len_indexes() {
            let index = unsafe{p.index(i)};
            match index.block_maybe() {
                Some(block) => o = write!(f, "      {:<5}: {}\n", i, block),
                None => {},
            }
        }
        write!(f, "  * blocks_ptr: {:?}\n", p._blocks);
        o = write!(f, "  * Block Data (len: {}, used: {} remain: {})\n",
                     p.len_blocks(), p.blocks_used, p.blocks_remaining());
        unsafe {
            let ptr = p as *const RawPool;
            let mut block = (*ptr).first_block();
            let first_block = match block {
                Some(b) => b as *const Block as usize,
                None => 0,
            };
            while let Some(b) = block {
                o = write!(f, "      {:<5}: {}\n",
                           (b as usize - first_block) / mem::size_of::<Block>(),
                           *b);
                block = (*b).next_mut(&*ptr).map(|b| b as *mut Block);
            }
        }
        o = write!(f, "      {:<5}: HEAP\n", p.heap_block);
        o = write!(f, "  * Freed Bins (len: {}):\n",
                     p.freed_bins.len);
        unsafe {
            for b in 0..NUM_BINS {
                o = write!(f, "      [{}] bin {}: ", FreedBins::bin_repr(b), b);
                let mut freed = p.freed_bins.bins[b as usize].root(p);
                while let Some(fr) = freed {
                    o = write!(f, "{} ", fr.block());
                    freed = fr.next().map(|b| p.freed(b))
                }
                o = write!(f, "\n");
            }
        }
        o = write!(f, "}}");
        o
    }
}

impl RawPool {
    pub fn display<'a>(&'a self) -> DisplayRawPool<'a> {
        DisplayRawPool{pool: self}
    }

    // Public unsafe API

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
        self.indexes_used += 1;
        self.blocks_used += blocks;
        Ok(i)
    }

    /// dealoc an index from the pool, this WILL corrupt any data that was there
    pub unsafe fn dealloc_index(&mut self, i: index) {
        // get the size and location from the Index and clear it
        let block = self.index(i).block();
        *self.index_mut(i) = Index::default();

        // start setting up freed
        let freed = self.freed_mut(block) as *mut Free;
        (*freed)._blocks &= BLOCK_BITMAP;  // set first bit to 0, needed to read blocks()
        let blocks = (*freed).blocks();

        // check if the freed is actually at the end of allocated heap, if so
        // just move the heap back
        if self.heap_block - blocks == block {
            self.heap_block = block;
        } else {
            assert!(block < self.heap_block - blocks);
            (*freed)._block = block;
            (*freed)._next = BLOCK_NULL;
            let selfptr = self as *mut RawPool;
            (*selfptr).freed_bins.insert(self, &mut *freed);
        }

        // statistics cleanup
        self.indexes_used -= 1;
        self.blocks_used -= blocks;
    }
    // /// combine all contiguous freed blocks together.
    pub unsafe fn clean(&mut self) {
        fn full_fn(pool: &mut RawPool, last_freed: Option<*mut Free>, full: &mut Full)
                   -> Option<*mut Free> {None}
        utils::base_clean(self, &full_fn);
    }

    pub unsafe fn defrag(&mut self) {
        /// this is the function for handling when Full values are found
        /// it moves them backwards if they are unlocked
        fn full_fn(pool: &mut RawPool, last_freed: Option<*mut Free>, full: &mut Full)
                   -> Option<*mut Free> {
            unsafe {
                let poolptr = pool as *mut RawPool;
                match last_freed {
                    Some(ref free) => match full.is_locked() {
                        false => {
                            // found an unlocked full value and the last value is free -- move it!
                            // moving is surprisingly simple because:
                            //  - the free.blocks does not change, so it does not change bins
                            //  - the free.prev/next do not change
                            //  - only the locations of full and free change, requiring their
                            //      data, and the items pointing at them, to be updated
                            println!("### Moving Backwards:\n{}", pool.display());
                            let i = full.index();
                            let blocks = full.blocks();
                            println!("blocks={}", blocks);
                            let mut free_tmp = (**free).clone();
                            println!("before tmp: {:?}", free_tmp);
                            let fullptr: *mut Block = mem::transmute(full);  // note: consumes full

                            // perform the move of the data
                            ptr::copy(fullptr, (*free) as *mut Block, blocks as usize);
                            println!("after  tmp: {:?}", free_tmp);

                            // it would be bad if we tried to access these anymore
                            drop(free);
                            drop(fullptr);

                            // full's data was already copied (blocks + index), only need
                            // to update the static Index's data
                            (*poolptr).index_mut(i)._block = free_tmp.block();

                            // update the free block's location and get it
                            free_tmp._block += blocks;

                            // update the items pointing TO free
                            match free_tmp.prev() {
                                Some(p) => (*poolptr).freed_mut(p)._next = free_tmp.block(),
                                None => {
                                    // the previous item is a bin, so we have to
                                    // do surgery in the freed bins
                                    let freed_bins = &mut (*poolptr).freed_bins;
                                    let bin = freed_bins.get_insert_bin(
                                        free_tmp.blocks());
                                    freed_bins.bins[bin as usize]._root = free_tmp.block();
                                }
                            }
                            if let Some(n) = free_tmp.next() {
                                (*poolptr).freed_mut(n)._prev = free_tmp.block();
                            }

                            // actually store the new free in pool._blocks
                            let new_free_loc = (*poolptr).freed_mut(free_tmp._block);
                            *new_free_loc = free_tmp;

                            println!("new free: {:?}", new_free_loc);
                            // the new_free is the last_freed for the next cycle
                            Some(new_free_loc as *mut Free)
                        }
                        true => None, // locked full value, can't move
                    },
                    None => None,  // the previous value is not free, can't move
                }
            }
        }
        utils::base_clean(self, &full_fn);
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

    // semi-private unsafe API

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

    /// get the pointer to the first block
    pub unsafe fn first_block(&self) -> Option<*mut Block> {
        if self.heap_block == 0 {
            None
        } else {
            Some(self._blocks)
        }
    }

    pub unsafe fn block(&self, block: block) -> *mut Block {
        self._blocks.offset(block as isize)
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

    // private API

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

    /// get a freed value from the freed bins
    unsafe fn use_freed(&mut self, blocks: block) -> Option<block> {
        let selfptr = self as *mut RawPool;
        match (*selfptr).freed_bins.pop(self, blocks) {
            Some(f) => Some(f),
            None => None,
        }
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

    // assert that Free.blocks() DOESN'T cancel out the high bit (not necessary)
    let mut f = Free {
        _blocks: BLOCK_BITMAP,
        _block: 0,
        _prev: 0,
        _next: 0,
    };
    assert_eq!(f.blocks(), BLOCK_BITMAP);
    f._blocks = 42;
    assert_eq!(f.blocks(), 42);
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
        let poolptr = (&mut pool) as *mut RawPool;

        assert_eq!(pool.get_unused_index().unwrap(), 0);
        assert_eq!(pool.get_unused_index().unwrap(), 1);
        assert_eq!(pool.get_unused_index().unwrap(), 2);
        let mut times_allocated = 3;
        let mut used_indexes = 0;
        let mut blocks_allocated = 0;

        // allocate an index
        println!("allocate 1");
        assert_eq!(pool.freed_bins.len, 0);
        let i = pool.alloc_index(4).unwrap();
        times_allocated += 1;
        assert_eq!(i, times_allocated - 1);
        let block;
        {
            let index = (*(&pool as *const RawPool)).index(i);
            assert_eq!(index._block, 0);
            block = index.block();
            assert_eq!(pool.full(index.block()).blocks(), 4);
            assert_eq!(pool.full(index.block())._blocks, BLOCK_HIGH_BIT | 4);
        }

        // allocate another index and then free it, to show that the heap
        // just automatically get's reclaimed
        assert_eq!(pool.freed_bins.len, 0);
        let tmp_i = pool.alloc_index(100).unwrap();
        times_allocated += 1;
        pool.dealloc_index(tmp_i);
        assert_eq!(pool.freed_bins.len, 0);
        assert_eq!(pool.heap_block, 4);
        assert_eq!(pool.blocks_used, 4);

        // allocate an index so that the deallocated one doesn't
        // go back to the heap
        let i_a = pool.alloc_index(1).unwrap();
        times_allocated += 1;
        used_indexes += 1;
        blocks_allocated += 1;
        assert_eq!(i_a, times_allocated - 1);

        // deallocate an index
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
        times_allocated += 1;
        used_indexes += 1;
        blocks_allocated += 8;
        assert_eq!(i2, times_allocated - 1);
        let block2;
        {
            let index2 = (*(&pool as *const RawPool)).index(i2);
            assert_eq!(index2._block, 5);
            block2 = index2.block();
            assert_eq!(pool.full(index2.block()).blocks(), 8);
            assert_eq!(pool.full(index2.block())._blocks, BLOCK_HIGH_BIT | 8);
        }

        // allocate a 3rd index (that does fit in the first)
        println!("allocate 3");
        let i3 = pool.alloc_index(2).unwrap();
        assert_eq!(i3, 7);
        times_allocated += 1;
        used_indexes += 1;
        blocks_allocated += 8;
        assert_eq!(i3, times_allocated - 1);
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

        // allocate 3 indexes then free the first 2
        // then run the freeing clean
        assert_eq!(i3, 7);
        let allocs = (
            pool.alloc_index(4).unwrap(),
            pool.alloc_index(4).unwrap(),
            pool.alloc_index(4).unwrap());

        times_allocated += 3;
        used_indexes += 1;
        blocks_allocated += 4;
        let free_block = pool.index(allocs.0).block();
        pool.dealloc_index(allocs.0);
        pool.dealloc_index(allocs.1);


        println!("cleaning freed");
        println!("{}", pool.display());
        pool.clean();
        {
            let free = pool.freed(free_block);
            assert_eq!(free.blocks(), 8);
        }
        pool.clean();
        pool.clean();

        // defrag and make sure everything looks like how one would expect it
        println!("defragging");
        println!("{}", pool.display());
        pool.defrag();

        assert_eq!(pool.freed_bins.len, 0);
        assert_eq!(pool.blocks_used, blocks_allocated);
        assert_eq!(pool.indexes_used, used_indexes);
    }
}
