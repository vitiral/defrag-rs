use super::raw_pool::*;
use super::free::*;

/// base clean function which allows the user to add functionality
/// to what should be done when a Full block is encountered
pub unsafe fn base_clean(pool: &mut RawPool,
                         full_fn: &Fn(&mut RawPool, Option<*mut Free>, &mut Full)
                                      -> Option<*mut Free>) {
    println!("### Starting a base clean");
    let poolptr = pool as *mut RawPool;
    let mut block_maybe = (*poolptr).first_block();
    let mut last_freed: Option<*mut Free> = None;
    while let Some(mut block) = block_maybe {
        // println!("{}", pool.display());
        match last_freed {
            Some(ref last) => println!("utils: last={:?}", **last),
            None => println!("utils: last=None"),
        }
        last_freed = match (*block).ty() {
            BlockType::Free => {
                let free = (*block).as_free_mut();
                println!("  block={:>3} {:?}", (*block).block(pool), free);
                match last_freed {
                    Some(ref last) => {
                        // combines the last with the current
                        // and set last_freed to the new value
                        println!("joining {:?} with {:?}", **last, free);
                        Some((**last).join(pool, free))
                    },
                    None => {
                        // last_freed is None, cannot combine
                        // but this is the new "last block"
                        Some(free as *mut Free)
                    }
                }
            },
            BlockType::Full => {
                let full = (*block).as_full_mut();
                println!("  block={:?}", full);
                last_freed = full_fn(pool, last_freed, full);
                if let Some(ref last) = last_freed {
                    // data was swapped, last freed is already the "next" block
                    block = pool.block((**last).block());
                }
                last_freed
            }
        };
        block_maybe = match (*block).next_mut(pool) {
            Some(b) => Some(b as *mut Block),
            None => None,
        };
    }
    if let Some(ref last) = last_freed {
        // there is freed data before the heap
        assert_eq!((**last).block() + (**last).blocks(), pool.heap_block);
        pool.heap_block -= (**last).blocks();
        (**last).remove(pool);
    }
}
