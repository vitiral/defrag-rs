
use super::types::*;
use super::pool::*;
use super::free::*;

/// base clean function which allows the user to add functionality
/// to what should be done when a Full block is encountered
pub unsafe fn base_clean(pool: &mut RawPool,
                         full_fn: &Fn(&mut RawPool, Option<*mut Free>, &mut Full)
                                      -> Option<*mut Free>) {
    let poolptr = pool as *mut RawPool;
    let mut block_maybe = (*poolptr).first_block();
    let mut last_freed: Option<*mut Free> = None;
    while let Some(block) = block_maybe {
        last_freed = match (*block).ty() {
            BlockType::Free => {
                let free = (*block).as_free_mut();
                // println!("{:?}", free);
                match last_freed {
                    Some(ref last) => {
                        // combines the last with the current
                        // and set last_freed to the new value
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
                // println!("{:?}", full);
                full_fn(pool, last_freed, full)
            }
        };
        block_maybe = match (*block).next_mut(pool) {
            Some(b) => Some(b as *mut Block),
            None => None,
        };
    }
}
