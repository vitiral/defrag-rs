//! contains all the logic related to the pool
//! these are the "blocks" of the pool

use super::types::*;
use core::mem;

#[derive(Copy, Clone)]
struct Index {
    _block: block,
}

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

    unsafe fn full(&self, pool: &Pool) -> &Full {
        assert!(self._block != NULL_BLOCK);
        Full::from_block(pool, self._block)
    }

    unsafe fn ptr(&self, pool: &Pool) -> *const u8 {
        assert!(self._block != NULL_BLOCK);
        let free = &pool.blocks[self._block];
        let mut ptr: *const u8 = mem::transmute(free);
        ptr = ptr.offset(mem::size_of::<Full>() as isize);
        ptr
    }
}

struct Pool {
    indexes: [Index; 256],   // actual pointers to the data
    last_index_used: usize,  // for speeding up finding indexes
    _freed: block,           // free values
    heap_block: block,       // the current location of the "heap"
    total_used: block,       // total memory currently used
    blocks: [Free; 4096],    // actual data
}

impl Pool {
    fn new() -> Pool {
        Pool {
            last_index_used: 0,
            indexes: [Index::default(); 256],
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
            if index._block != NULL_BLOCK {
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
    fn alloc_index(&mut self, blocks: block) -> Result<index> {
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
    unsafe fn dealloc_index(&mut self, i: index) {
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

/// the Free struct is a linked list of free values with
/// the root as a size-bin in pool
#[repr(packed)]
#[derive(Default, Copy, Clone)]
struct Free {
    // The first bit of blocks is always 0 for Free structs
    _blocks: block,        // size of this freed memory
    block: block,          // block location of this struct
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
    _index: index,         // data which contains the index and the lock information
    // space after this (until block + blocks) is invalid
}

impl Full {
    unsafe fn from_block_mut(pool: &mut Pool, block: block) -> &mut Full {
        let free = &mut pool.blocks[block];
        mem::transmute(free)
    }

    unsafe fn from_block(pool: &Pool, block: block) -> &Full {
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
