//! contains all the logic related to the pool
//! these are the "blocks" of the pool

use core;
use core::mem;
use core::default::Default;
use core::slice;

use super::types::*;

// ##################################################
// # Struct Definitions
#[repr(packed)]
#[derive(Copy, Clone)]
pub struct Block {
    _data: Free,
}

impl Block {
    pub fn dumb(&self) -> &'static str {
        "what"
    }
}

impl Default for Block {
    fn default() -> Block {
        Block {_data: Free {_blocks: 0, block: 0, _prev: 0, _next: 0}}
    }
}

#[repr(packed)]
#[derive(Debug, Copy, Clone)]
pub struct Index {
    __block: block,
}

/// The RawPool is the private container and manager for all allocated
/// and freed data
// TODO: indexes and blocks need to be dynamically sized
pub struct RawPool {
    last_index_used: usize,  // for speeding up finding indexes
    _freed: block,           // free values
    heap_block: block,       // the current location of the "heap"
    total_used: block,       // total memory currently used
    _indexes: *mut Index,    // does not move and stores movable block location of data
    _indexes_len: index,     // len of indexes
    _blocks: *mut Block,     // actual data
    _blocks_len: block,      // len of blocks
}

/// the Free struct is a linked list of free values with
/// the root as a size-bin in pool
#[repr(packed)]
#[derive(Debug, Copy, Clone)]
pub struct Free {
    // NOTE: DO NOT MOVE `_blocks`, IT IS SWAPPED WITH `_blocks` IN `Full`
    // The first bit of `_blocks` is always 0 for Free structs
    _blocks: block,        // size of this freed memory
    block: block,          // block location of this struct
    _prev: block,          // location of previous freed memory
    _next: block,          // location of next freed memory
    // data after this (until block + blocks) is invalid
}

/// the Full struct only contains enough information about the data to
/// allocate it
#[repr(packed)]
#[derive(Debug)]
pub struct Full {
    // NOTE: DO NOT MOVE `_blocks`, IT IS SWAPPED WITH `_blocks` IN `Free`
    // The first bit of blocks is 1 for Full structs
    _blocks: block,        // size of this freed memory
    _index: index,         // data which contains the index and the lock information
    // space after this (until block + blocks) is invalid
}


// ##################################################
// # Index impls

impl Default for Index {
    fn default() -> Index {
        Index {__block: BLOCK_NULL}
    }
}

impl Index {
    /// get the block where the index is stored
    pub fn block(&self) -> block {
        self.__block
    }

    /// get size of Index DATA in bytes
    pub fn size(&self, pool: &RawPool) -> usize {
        unsafe {
            pool.full(self.block()).blocks() - mem::size_of::<Full>()
        }
    }
}

// ##################################################
// # Free impls

impl Default for Free {
    fn default() -> Free {
        Free {
            _blocks: 0,
            block: BLOCK_NULL,
            _prev: BLOCK_NULL,
            _next: BLOCK_NULL,
        }
    }
}

impl Free {
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
                self._next = n.block;
                n._prev = self.block;
            }
            None => self._next = BLOCK_NULL,
        }
    }

    /// set the prev freed block and set it's next to self
    unsafe fn set_prev(&mut self, pool: &mut RawPool, prev: Option<&mut Free>) {
        match prev {
            Some(p) => {
                self._prev = p.block;
                p._next = self.block;
            }
            None => {
                pool.set_freed_bin(Some(self));
            }
        }
    }

    /// append a freed block after this one
    pub fn append(&mut self, pool: &mut RawPool, next: &mut Free) {
        let pool = pool as *mut RawPool;
        unsafe {
            if let Some(n) = self.next() {
                (*pool).freed_mut(n).set_prev(&mut (*pool), Some(next));
            }
            self.set_next(Some(next));
        }
    }

    /// remove self from the freed pool
    pub fn remove(&mut self, pool: &mut RawPool) {
        /// convinience function for this method only
        fn get_freed<'a>(pool: &'a mut RawPool, block: Option<block>) -> Option<&'a mut Free> {
            match block {
                Some(b) => {
                    assert!(b < pool.len_blocks() - 1);
                    Some(unsafe{pool.freed_mut(b)})
                }
                None => None,
            }
        }
        let poolp = pool as *mut RawPool;
        unsafe {
            match get_freed(&mut *poolp, self.prev()) {
                Some(p) => p.set_next(get_freed(&mut *poolp, self.next())),
                None => (*poolp).set_freed_bin(get_freed(&mut *poolp, self.next())),
            }
        }
    }
}

// ##################################################
// # Full impls

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
// # RawPool impls

impl RawPool {
    pub fn new(indexes: *mut Index, indexes_len: index,
               blocks: *mut Block, blocks_len: block)
               -> RawPool {
        unsafe {
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
        }

        RawPool {
            last_index_used: indexes_len - 1,
            _freed: BLOCK_NULL,
            heap_block: 0,
            total_used: 0,
            _indexes: indexes,
            _indexes_len: indexes_len,
            _blocks: blocks,
            _blocks_len: blocks_len,

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
        self.len_blocks() * mem::size_of::<Block>()
    }

    /// raw blocks remaining (fragmented or not)
    pub fn blocks_remaining(&self) -> block {
        self.len_blocks() - self.total_used
    }

    /// raw bytes remaining (fragmented or not)
    pub fn bytes_remaining(&self) -> block {
        self.blocks_remaining() * mem::size_of::<Block>()
    }

    /// get the index
    pub fn index(&self, i: index) -> &Index {
        unsafe {
            let ptr = self._indexes.offset(i as isize);
            mem::transmute(ptr)
        }
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
    pub fn freed(&self, block: block) -> &Free {
        unsafe {
            let ptr = self._blocks.offset(block as isize);
            mem::transmute(ptr)
        }
    }

    /// mut the block as a Free block
    pub unsafe fn freed_mut(&self, block: block) -> &mut Free {
        let ptr = self._blocks.offset(block as isize);
        mem::transmute(ptr)
    }

    /// read the block as a Full block
    pub fn full(&self, block: block) -> &Full {
        unsafe {
            let ptr = self._blocks.offset(block as isize);
            mem::transmute(ptr)
        }
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
            if self.index(i).__block == BLOCK_NULL {
                self.last_index_used = i;
                return Ok(i);
            }
            i = (i + 1) % self.len_indexes();
        }
        return Err(Error::OutOfIndexes);
    }

    fn freed_bin(&self) -> Option<block> {
        if self._freed == BLOCK_NULL {
            None
        } else {
            Some(self._freed)
        }
    }

    fn set_freed_bin(&mut self, free: Option<&mut Free>) {
        match free {
            Some(f) => {
                self._freed = f.block;
                f._prev = BLOCK_NULL;  // it is the beginning of the list
            }
            None => self._freed = BLOCK_NULL,
        }
    }

    fn get_freed_block(&mut self, blocks: block) -> Option<block> {
        let mut block = match self.freed_bin() {
            Some(b) => b,
            None => return None,
        };
        loop {
            unsafe {
                let freed = self.freed_mut(block) as *mut Free;
                // all unsafe operations are safe because we know that we are
                // never changing more than one freed block at a time
                if (*freed).blocks() == blocks {
                    // perfectly equal, consumes freed block
                    (*freed).remove(self);
                    return Some((*freed).block);
                } else if (*freed).blocks() > blocks {
                    // use only the size that is needed, so append a new freed block
                    let new_freed = self.freed_mut((*freed).block + blocks)
                        as *mut Free;
                    (*new_freed) = Free {
                        _blocks: (*freed).blocks() - blocks,
                        block: (*freed).block + blocks,
                        _prev: BLOCK_NULL,
                        _next: BLOCK_NULL,
                    };
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

    /// allocate data with a specified number of blocks,
    /// including the half block required to store `Full` struct
    pub fn alloc_index(&mut self, blocks: block) -> Result<index> {
        if blocks == 0 {
            return Err(Error::InvalidSize);
        }
        if self.blocks_remaining() < blocks {
            return Err(Error::OutOfMemory);
        }
        let prev_used_index = self.last_index_used;
        let i = try!(self.get_unused_index());
        let block = if let Some(block) = self.get_freed_block(blocks) {
            // we are reusing previously freed data
            block
        } else if (self.heap_block + blocks) as usize <= self.len_blocks() {
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
            index.__block = block
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
        // get the size and location from the Index and clear it
        let block = self.index(i).block();
        let freed = self.freed_mut(block) as *mut Free;
        (*freed)._blocks &= BLOCK_BITMAP;  // set first bit to 0
        (*freed).block = block;
        *self.index_mut(i) = Index::default();
        self.set_freed_bin(Some(&mut *freed));
    }
}

// Tests

#[test]
fn test_basic() {
    // assert that our bitmap is as expected
    let highbit = 1 << ((mem::size_of::<block>() * 8) - 1);
    assert_eq!(BLOCK_BITMAP, !highbit, "{:b} != {:b}", BLOCK_BITMAP, !highbit);
    assert_eq!(BLOCK_HIGH_BIT, highbit, "{:b} != {:b}", BLOCK_HIGH_BIT, highbit);

    // assert that Full.blocks() cancels out the high bit
    let expected = highbit ^ usize::max_value();
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

#[test]
/// test using raw indexes
fn test_indexes() {
    let mut indexes = [Index::default(); 256];
    let mut blocks = [Block::default(); 4096];
    let len_i = indexes.len();
    let iptr: *mut Index = unsafe { mem::transmute(&mut indexes[..][0]) };
    let len_b = blocks.len();
    let bptr: *mut Block = unsafe { mem::transmute(&mut blocks[..][0]) };

    let mut pool = RawPool::new(iptr, len_i, bptr, len_b);
    assert_eq!(pool.get_unused_index().unwrap(), 0);
    assert_eq!(pool.get_unused_index().unwrap(), 1);
    assert_eq!(pool.get_unused_index().unwrap(), 2);

    // allocate an index
    println!("allocate 1");
    assert_eq!(pool.freed_bin(), None);
    let i = pool.alloc_index(4).unwrap();
    assert_eq!(i, 3);
    let block;
    {
        let index = unsafe {(*(&pool as *const RawPool)).index(i)};
        assert_eq!(index.__block, 0);
        block = index.block();
        unsafe {
            assert_eq!(pool.full(index.block()).blocks(), 4);
            assert_eq!(pool.full(index.block())._blocks, BLOCK_HIGH_BIT | 4);
        }
    }

    // deallocate the index
    println!("free 1");
    assert_eq!(pool.freed_bin(), None);
    unsafe {
        pool.dealloc_index(i);
    }
    {
        assert_eq!(pool.index(i).__block, BLOCK_NULL);

        let freed = pool.freed(block);
        assert_eq!(freed.blocks(), 4);
        assert_eq!(freed._blocks, 4);
        assert_eq!(freed.prev(), None);
        assert_eq!(freed.next(), None);
        assert_eq!(pool.freed_bin().unwrap(), block);
    }

    // allocate another index
    println!("allocate 2");
    let i2 = pool.alloc_index(8).unwrap();
    assert_eq!(i2, 4);
    let block2;
    {
        let index2 = unsafe {(*(&pool as *const RawPool)).index(i2)};
        assert_eq!(index2.__block, 4);
        block2 = index2.block();
        unsafe {
            assert_eq!(pool.full(index2.block()).blocks(), 8);
            assert_eq!(pool.full(index2.block())._blocks, BLOCK_HIGH_BIT | 8);
        }
    }

    // allocate a 3rd index that fits in the first
    println!("allocate 3");
    let i3 = pool.alloc_index(2).unwrap();
    assert_eq!(i3, 5);
    let block3;
    {

        let index3 = unsafe {(*(&pool as *const RawPool)).index(i3)};
        assert_eq!(index3.__block, 0);
        block3 = index3.block();
        unsafe {
            assert_eq!(pool.full(index3.block()).blocks(), 2);
            assert_eq!(pool.full(index3.block())._blocks, BLOCK_HIGH_BIT | 2);
        }
    }
    // tests related to the fact that i3 just overwrote the freed item
    let free_block = pool.freed_bin().unwrap();
    assert_eq!(free_block, 2);
    {
        // free1 has moved
        let free = pool.freed(free_block);
        assert_eq!(free.blocks(), 2);
        assert_eq!(free.prev(), None);
        assert_eq!(free.next(), None);
    }
}
