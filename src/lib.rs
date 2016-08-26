#![no_std]

use core::result;

type index = u16;
type block = u16;

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

/// the Freed struct is a linked list of values
#[repr(C)]
struct Freed {
    block: block,         // block location of this struct
    size: block,          // size of this freed memory
    prev: Option<block>,  // location of previous freed memory
    next: Option<block>,  // location of next freed memory
}

/// the index is how the application finds the data
/// and frees it
#[derive(Default)]
struct Index {
    size: block,   // the size of the data in blocks, 0 if not used
    block: block,  // the block where the data is located
}

/// memory error codes
enum Error {
    Fragmented,
    OutOfMemory,
    OutOfIndexes,
    InvalidSize,
}

type Result<T> = result::Result<T, Error>;


/// get an unused index
fn get_unused_index(pool: &mut Pool) -> Result<index> {
    // TODO:
    // this is currently pretty slow -- there are some things
    // that we could do to make it faster, like keep an array
    // of unused indexes that get repopulated when there is
    // nothing to defragment, or keep a binary list so we can
    // search for the unused index by usize increments... or both
    let mut i = (pool.last_used_index + 1) % pool.indexes.len() as u16;
    while i != pool.last_used_index {
        let index = &pool.indexes[i as usize];
        if index.size != 0 {
            pool.last_used_index = i;
            return Ok(i as index);
        }
        i = (i + 1) % pool.indexes.len() as u16;
    }
    return Err(Error::OutOfIndexes);
}

/// iterate through the freed linked-list to find a freed index
/// of the appropriate size.
/// This can be sped up dramatically with a hash table
fn get_freed_block(pool: &mut Pool, size: block) -> Option<block> {
    let mut block = match pool.freed {
        Some(b) => b,
        None => return None,
    };
    loop {
        let freed = &mut pool.blocks[block as usize] as *mut Freed;
        // unsafe because we need a reference (*freed) inside pool.blocks while
        // we mutate data in pool.blocks
        unsafe {
            if (*freed).size == size {
                // perfectly equal, consumes freed block
                match (*freed).prev {
                    Some(p) => pool.blocks[p as usize].next = (*freed).next,
                    None => pool.freed = (*freed).next,
                }
                match (*freed).next {
                    Some(p) => pool.blocks[p as usize].prev = (*freed).prev,
                    None => {},
                }
                return Some((*freed).block);
            } else if (*freed).size > size {
                // use only the size that is needed, so insert a new freed block
                let new_block = (*freed).block + size;
                {
                    let new_freed = &mut pool.blocks[new_block as usize];
                    new_freed.prev = (*freed).prev;
                    new_freed.next = (*freed).next;
                }
                let new_block = Some(new_block);
                match (*freed).prev {
                    Some(p) => pool.blocks[p as usize].next = new_block,
                    None => pool.freed = new_block,
                }
                match (*freed).next {
                    Some(p) => pool.blocks[p as usize].prev = new_block,
                    None => {},
                }
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
fn alloc_index(pool: &mut Pool, size: block) -> Result<index> {
    if size == 0 {
        return Err(Error::InvalidSize);
    }
    if (pool.total_used + size) as usize > pool.blocks.len() {
        return Err(Error::OutOfMemory);
    }
    let prev_used_index = pool.last_used_index;
    let index = try!(get_unused_index(pool));
    if let Some(block) = get_freed_block(pool, size) {
        pool.indexes[index as usize].size = size;
        pool.indexes[index as usize].block = block;
        Ok(index)
    } else if (pool.heap_block + size) as usize <= pool.blocks.len() {
        pool.indexes[index as usize].size = size;
        pool.indexes[index as usize].block = pool.heap_block;
        pool.heap_block += size;
        Ok(index)
    } else {
        pool.last_used_index = prev_used_index;
        Err(Error::Fragmented)
    }
}


fn dealloc_index(pool: &mut Pool, i: index) {
    // get the size and location from the Index and clear it
    let block = pool.indexes[i as usize].block;
    let freed = &mut pool.blocks[block as usize];
    freed.size = pool.indexes[i as usize].size;
    pool.indexes[i as usize] = Index::default();

    // update the linked list and data
    freed.next = pool.freed;
    freed.prev = None;
    freed.block = block;
    pool.freed = Some(block);
}
