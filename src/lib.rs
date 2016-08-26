#![no_std]

use core::result;

/// memory error codes
enum Error {
    Fragmented,
    OutOfMemory,
    OutOfIndexes,
    InvalidSize,
}

type Result<T> = result::Result<T, Error>;


type index = u16;
type block = u16;


/// the Freed struct is a linked list of values
#[repr(C)]
struct Freed {
    block: block,         // block location of this struct
    size: block,          // size of this freed memory
    prev: Option<block>,  // location of previous freed memory
    next: Option<block>,  // location of next freed memory
}

impl Freed {
    /// set the next freed block and set it's prev to self
    unsafe fn set_next(&mut self, next: Option<&mut Freed>) {
        match next {
            Some(n) => {
                self.next = Some(n.block);
                n.prev = Some(self.block);
            }
            None => self.next = None,
        }
    }

    /// set the prev freed block and set it's next to self
    unsafe fn set_prev(&mut self, pool: &mut Pool, prev: Option<&mut Freed>) {
        match prev {
            Some(p) => {
                self.prev = Some(p.block);
                p.next = Some(self.block);
            }
            None => {
                pool.set_freed(Some(self));
            }
        }
    }

    /// append a freed block after this one
    fn append(&mut self, pool: &mut Pool, next: &mut Freed) {
        let pool = pool as *mut Pool;
        unsafe {
            if let Some(n) = self.next {
                (*pool).blocks[n as usize].set_prev(&mut (*pool), Some(next));
            }
            self.set_next(Some(next));
        }
    }

    /// remove self from the freed pool
    fn remove(&mut self, pool: &mut Pool) {
        let poolp = pool as *mut Pool;
        unsafe {
            match (*poolp).get_freed(self.prev) {
                Some(p) => p.set_next(pool.get_freed(self.next)),
                None => (*poolp).set_freed(pool.get_freed(self.next)),
            }
        }
    }
}

/// the index is how the application finds the data
/// and frees it
#[derive(Default)]
struct Index {
    size: block,   // the size of the data in blocks, 0 if not used
    block: block,  // the block where the data is located
}
/// The pool contains all the information necessary to
/// allocate and reserve data
struct Pool {
    last_used_index: index,
    indexes: [Index; 128],
    freed: Option<block>,
    heap_block: block,
    total_used: block,
    blocks: [Freed; 4096],
}

impl Pool {
    fn get_freed(&mut self, b: Option<block>) -> Option<&mut Freed> {
        match b {
            Some(b) => Some(&mut self.blocks[b as usize]),
            None => None,
        }
    }

    fn set_freed(&mut self, free: Option<&mut Freed>) {
        match free {
            Some(f) => {
                self.freed = Some(f.block);
                f.prev = None;  // it is the beginning of the list
            }
            None => self.freed = None,
        }
    }

    fn insert_freed(&mut self, free: &mut Freed) {
        if let Some(f) = self.freed {
            self.blocks[f as usize].prev = Some(free.block);
        }
        self.set_freed(Some(free));
    }

    /// get an unused index
    fn get_unused_index(&mut self) -> Result<index> {
        // TODO:
        // this is currently pretty slow -- there are some things
        // that we could do to make it faster, like keep an array
        // of unused indexes that get repopulated when there is
        // nothing to defragment, or keep a binary list so we can
        // search for the unused index by usize increments... or both
        let mut i = (self.last_used_index + 1) % self.indexes.len() as u16;
        while i != self.last_used_index {
            let index = &self.indexes[i as usize];
            if index.size != 0 {
                self.last_used_index = i;
                return Ok(i as index);
            }
            i = (i + 1) % self.indexes.len() as u16;
        }
        return Err(Error::OutOfIndexes);
    }

    fn get_freed_block(&mut self, size: block) -> Option<block> {
        let mut block = match self.freed {
            Some(b) => b,
            None => return None,
        };
        loop {
            let freed = &mut self.blocks[block as usize] as *mut Freed;
            unsafe {
                // all unsafe operations are safe because we know that we are
                // never changing more than one freed block at a time
                if (*freed).size == size {
                    // perfectly equal, consumes freed block
                    (*freed).remove(self);
                    return Some((*freed).block);
                } else if (*freed).size > size {
                    // use only the size that is needed, so append a new freed block
                    let new_freed = &mut self.blocks[((*freed).block + size) as usize]
                        as *mut Freed;
                    (*freed).append(self, &mut (*new_freed));
                    (*freed).remove(self);
                    return Some((*freed).block);
                }
                block = match (*freed).next {
                    Some(b) => b,
                    None => return None,
                };
            }
        }
    }
    /// allocate data of a certain size, returning it's index
    /// in the pool.blocks
    fn alloc_index(&mut self, size: block) -> Result<index> {
        if size == 0 {
            return Err(Error::InvalidSize);
        }
        if (self.total_used + size) as usize > self.blocks.len() {
            return Err(Error::OutOfMemory);
        }
        let prev_used_index = self.last_used_index;
        let index = try!(self.get_unused_index());
        if let Some(block) = self.get_freed_block(size) {
            self.indexes[index as usize].size = size;
            self.indexes[index as usize].block = block;
            Ok(index)
        } else if (self.heap_block + size) as usize <= self.blocks.len() {
            self.indexes[index as usize].size = size;
            self.indexes[index as usize].block = self.heap_block;
            self.heap_block += size;
            Ok(index)
        } else {
            self.last_used_index = prev_used_index;
            Err(Error::Fragmented)
        }
    }

    /// dealoc an index from the pool, this will corrupt any data that was there
    unsafe fn dealloc_index(&mut self, i: index) {
        // get the size and location from the Index and clear it
        let freed = {
            let ref index = self.indexes[i as usize];
            assert!(index.size != 0, "size={}", index.size);
            &mut self.blocks[index.block as usize]
        } as *mut Freed;
        // let index = &self.indexes[i as usize] as *const Index;
        // assert!((*index).size != 0, "size={}", (*index).size);
        // let freed = &mut self.blocks[(*index).block as usize] as *mut Freed;
        (*freed).block = self.indexes[i as usize].block;
        (*freed).size = self.indexes[i as usize].size;
        self.indexes[i as usize] = Index::default();
        self.set_freed(Some(&mut *freed));
    }
}



