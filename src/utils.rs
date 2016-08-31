
use super::types::*;
use super::pool::*;
use super::free::*;

pub unsafe fn base_clean(pool: &mut RawPool,
                         full_fn: &Fn(&mut RawPool, Option<*mut Free>)
                         -> Option<*mut Free>) {
    // this is very similar in logic to free.rs::FreeBin.clean
    // not sure if that logic can be easily combined though...
    let poolptr = pool as *mut RawPool;
    let mut block_maybe = (*poolptr).first_block();
    let mut last_freed: Option<*mut Free> = None;
    while let Some(block) = block_maybe {
        last_freed = match (*block).ty() {
            BlockType::Free => match last_freed {
                Some(ref last) => {
                    // combines the last with the current
                    // and set last_freed to the new value
                    Some((**last).join(pool, (*block).as_free_mut()))
                },
                None => {
                    // last_freed is None, cannot combine
                    // but this is the new "last block"
                    Some((*block).as_free_mut() as *mut Free)
                }
            },
            BlockType::Full => full_fn(pool, last_freed),
        };
        block_maybe = match (*block).next_mut(pool) {
            Some(b) => Some(b as *mut Block),
            None => None,
        };
    }
}
