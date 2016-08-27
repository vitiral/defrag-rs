//! contains all the logic related to the pool
//! these are the "blocks" of the pool

use super::types::*;
use core::mem;

// ##################################################
// # Struct Definitions

#[derive(Debug, Copy, Clone)]
struct Index {
    _block: block,
}

/// The RawPool is the private container and manager for all allocated
/// and freed data
// TODO: indexes and blocks need to be dynamically sized
struct RawPool<'a> {
    indexes: &'a [Index],   // actual pointers to the data
    last_index_used: usize,  // for speeding up finding indexes
    _freed: block,           // free values
    heap_block: block,       // the current location of the "heap"
    total_used: block,       // total memory currently used
    blocks: [Free; 4096],    // actual data
}

/// the Free struct is a linked list of free values with
/// the root as a size-bin in pool
#[repr(packed)]
#[derive(Debug, Copy, Clone)]
struct Free {
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
struct Full {
    // NOTE: DO NOT MOVE `_blocks`, IT IS SWAPPED WITH `_blocks` IN `Free`
    // The first bit of blocks is 1 for Full structs
    _blocks: block,        // size of this freed memory
    _index: index,         // data which contains the index and the lock information
    // space after this (until block + blocks) is invalid
}


// ##################################################
// # Index impls

impl core::default::Default for Index {
    fn default() -> Index {
        Index {_block: NULL_BLOCK}
    }
}

impl Index {
    fn block(&self) -> Option<block> {
        if self._block == NULL_BLOCK {
            None
        } else {
            Some(self._block)
        }
    }

    unsafe fn full<'a>(&self, pool: &'a RawPool) -> &'a Full {
        let block = match self.block() {
            Some(b) => b,
            None => unreachable!(),
        };
        Full::from_block(pool, block)
    }

    unsafe fn ptr(&self, pool: &RawPool) -> *const u8 {
        let block = match self.block() {
            Some(b) => b,
            None => unreachable!(),
        };
        let free = &pool.blocks[block];
        let mut ptr: *const u8 = mem::transmute(free);
        ptr = ptr.offset(mem::size_of::<Full>() as isize);
        ptr
    }
}

// ##################################################
// # Free impls

impl Default for Free {
    fn default() -> Free {
        Free {
            _blocks: 0,
            block: NULL_BLOCK,
            _prev: NULL_BLOCK,
            _next: NULL_BLOCK,
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
        if self._prev == NULL_BLOCK {
            None
        } else {
            Some(self._prev)
        }
    }

    /// next accessor, handling any bitmaps
    fn next(&self) -> Option<block> {
        if self._next == NULL_BLOCK {
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
            None => self._next = NULL_BLOCK,
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
                pool.set_freed(Some(self));
            }
        }
    }

    /// append a freed block after this one
    pub fn append(&mut self, pool: &mut RawPool, next: &mut Free) {
        let pool = pool as *mut RawPool;
        unsafe {
            if let Some(n) = self.next() {
                (*pool).blocks[n as usize].set_prev(&mut (*pool), Some(next));
            }
            self.set_next(Some(next));
        }
    }

    /// remove self from the freed pool
    pub fn remove(&mut self, pool: &mut RawPool) {
        let poolp = pool as *mut RawPool;
        unsafe {
            match (*poolp).get_freed(self.prev()) {
                Some(p) => p.set_next(pool.get_freed(self.next())),
                None => (*poolp).set_freed(pool.get_freed(self.next())),
            }
        }
    }
}

// ##################################################
// # Full impls

impl Full {
    unsafe fn from_block_mut(pool: &mut RawPool, block: block) -> &mut Full {
        let free = &mut pool.blocks[block];
        mem::transmute(free)
    }

    unsafe fn from_block(pool: &RawPool, block: block) -> &Full {
        let free = &pool.blocks[block];
        mem::transmute(free)
    }

    fn blocks(&self) -> block {
        self._blocks & BLOCK_BITMAP
    }

    fn index(&self) -> block {
        self._index & INDEX_BITMAP
    }

    fn locked(&self) -> bool {
        self._index & HIGH_INDEX_BIT == HIGH_INDEX_BIT
    }
}

// ##################################################
// # RawPool impls

impl RawPool {
    fn new() -> RawPool {
        let indexes = [Index::default(); 256];
        RawPool {
            last_index_used: indexes.len() - 1,
            indexes: indexes,
            _freed: NULL_BLOCK,
            heap_block: 0,
            total_used: 0,
            blocks: [Free::default(); 4096],
        }
    }

    /// get an unused index
    fn get_unused_index(&mut self) -> Result<index> {
        // TODO: this is currently pretty slow, maybe cache freed indexes?
        let mut i = (self.last_index_used + 1) % self.indexes.len();
        while i != self.last_index_used {
            let index = &self.indexes[i];
            if index.block() == None {
                self.last_index_used = i;
                return Ok(i);
            }
            i = (i + 1) % self.indexes.len();
        }
        return Err(Error::OutOfIndexes);
    }

    fn freed(&self) -> Option<block> {
        if self._freed == NULL_BLOCK {
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
                f._prev = NULL_BLOCK;  // it is the beginning of the list
            }
            None => self._freed = NULL_BLOCK,
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
                    (*new_freed) = Free {
                        _blocks: (*freed).blocks() - blocks,
                        block: (*freed).block + blocks,
                        _prev: NULL_BLOCK,
                        _next: NULL_BLOCK,
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
        if (self.total_used + blocks) as usize > self.blocks.len() {
            return Err(Error::OutOfMemory);
        }
        let prev_used_index = self.last_index_used;
        let i = try!(self.get_unused_index());
        let block = if let Some(block) = self.get_freed_block(blocks) {
            block
        } else if (self.heap_block + blocks) as usize <= self.blocks.len() {
            let block = self.heap_block;
            self.heap_block += blocks;
            block
        } else {
            self.last_index_used = prev_used_index;
            return Err(Error::Fragmented)
        };
        // set the index data
        {
            let index = &mut self.indexes[i];
            index._block = block
        }
        // set the full data in the block
        {
            let full = unsafe{Full::from_block_mut(self, block)};
            full._blocks = blocks | HIGH_BLOCK_BIT;
            full._index = i;  // starts unlocked
        }
        Ok(i)
    }

    /// dealoc an index from the pool, this WILL corrupt any data that was there
    pub unsafe fn dealloc_index(&mut self, i: index) {
        // get the size and location from the Index and clear it
        let block = match self.indexes[i].block() {
            Some(b) => b,
            None => unreachable!(),
        };
        let freed = &mut self.blocks[block] as *mut Free;
        (*freed)._blocks &= BLOCK_BITMAP;  // set first bit to 0
        (*freed).block = block;
        self.indexes[i] = Index::default();
        self.set_freed(Some(&mut *freed));
    }
}

// Tests

#[test]
fn test_basic() {
    // assert that our bitmap is as expected
    let highbit = 1 << ((mem::size_of::<block>() * 8) - 1);
    assert_eq!(BLOCK_BITMAP, !highbit, "{:b} != {:b}", BLOCK_BITMAP, !highbit);
    assert_eq!(HIGH_BLOCK_BIT, highbit, "{:b} != {:b}", HIGH_BLOCK_BIT, highbit);

    // assert that Full.blocks() cancels out the high bit
    let expected = highbit ^ usize::max_value();
    let mut f = Full {
        _blocks: NULL_BLOCK,
        _index: NULL_INDEX,
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
fn test_indexes() {
    // test using raw indexes
    let mut pool = RawPool::new();
    assert_eq!(pool.get_unused_index().unwrap(), 0);
    assert_eq!(pool.get_unused_index().unwrap(), 1);
    assert_eq!(pool.get_unused_index().unwrap(), 2);

    // allocate an index
    println!("allocate 1");
    assert_eq!(pool.freed(), None);
    let i = pool.alloc_index(4).unwrap();
    assert_eq!(i, 3);
    let block;
    {
        let index = &pool.indexes[i];
        block = index.block().unwrap();
        assert_eq!(block, 0);
        unsafe {
            assert_eq!(index.full(&pool).blocks(), 4);
            assert_eq!(index.full(&pool)._blocks, HIGH_BLOCK_BIT | 4);

        }
    }

    // deallocate the index
    println!("free 1");
    assert_eq!(pool.freed(), None);
    unsafe {
        pool.dealloc_index(i);
    }
    {
        assert_eq!(pool.indexes[i].block(), None);

        let freed = &pool.blocks[block];
        assert_eq!(freed.blocks(), 4);
        assert_eq!(freed._blocks, 4);
        assert_eq!(freed.prev(), None);
        assert_eq!(freed.next(), None);
        assert_eq!(pool.freed().unwrap(), block);
    }

    // allocate another index
    println!("allocate 2");
    let i2 = pool.alloc_index(8).unwrap();
    assert_eq!(i2, 4);
    let block2;
    {
        let index2 = &pool.indexes[i2];
        block2 = index2.block().unwrap();
        assert_eq!(block2, 4);
        unsafe {
            assert_eq!(index2.full(&pool).blocks(), 8);
            assert_eq!(index2.full(&pool)._blocks, HIGH_BLOCK_BIT | 8);
        }
    }

    // allocate a 3rd index that fits in the first
    println!("allocate 3");
    let i3 = pool.alloc_index(2).unwrap();
    assert_eq!(i3, 5);
    let block3;
    {
        let index3 = &pool.indexes[i3];
        block3 = index3.block().unwrap();
        assert_eq!(block3, 0);
        unsafe {
            assert_eq!(index3.full(&pool).blocks(), 2);
            assert_eq!(index3.full(&pool)._blocks, HIGH_BLOCK_BIT | 2);
        }
    }
    // tests related to the fact that i3 just overwrote the freed item
    let free_block = pool.freed().unwrap();
    assert_eq!(free_block, 2);
    {
        // free1 has moved
        let free = pool.blocks[free_block];
        assert_eq!(free.blocks(), 2);
        assert_eq!(free.prev(), None);
        assert_eq!(free.next(), None);
    }
}
