use core;

use super::pool::{RawPool, Block, Index, Full};
use core::mem;
use core::marker::PhantomData;
use core::ops::Deref;

type Result<T> = core::result::Result<T, ()>;
type TryLockResult<T> = core::result::Result<T, ()>;

struct Pool<'a> {
    raw: RawPool<'a>,
}

impl<'a> Pool<'a> {
    pub fn new(indexes:&'a mut [Index], blocks: &'a mut [Block]) -> Pool<'a> {
        Pool{raw: RawPool::new(indexes, blocks)}
    }
}

struct Mutex<'a, T> {
    index: usize,
    pool: &'a Pool<'a>,
    _type: PhantomData<T>,
}

impl<'pool, T: Sized> Mutex<'pool, T> {
    pub fn try_lock(&'pool self) -> TryLockResult<MutexGuard<T>> {
        // let index = self.pool.raw.indexes[self.index]
        // let full = self.pool.raw.full_mut(index.block());
        Ok(MutexGuard{__lock: self})
    }
}


struct MutexGuard<'a, T: Sized + 'a> {
    // Maybe remove this 'a?
    __lock: &'a Mutex<'a, T>,
}

impl <'pool> Pool<'pool> {
    pub fn alloc<T>(&'pool self, i: usize) -> Result<Mutex<'pool, T>> {
        // Note: in real-life this will fail if not enough mem
        Ok(Mutex{index: i, pool: self, _type: PhantomData})
    }
}

// maybe add + 'mutex here?
// impl<'mutex, T: Sized> Deref for MutexGuard<'mutex, T> {
//     type Target = T;

//     fn deref(&self) -> &T {
//         unsafe {
//             let p: *const u8 = mem::transmute(&self.__lock.pool.raw._raw_blocks[0]);
//             mem::transmute(p)
//         }
//     }
// }

#[test]
fn it_works() {
    // let mut indexes = [Index::default(); 256];
    // let mut blocks = [Block::default(); 4096];
    // let mut pool = Pool::new(&mut indexes, &mut blocks);
    // let alloc = pool.alloc::<u32>(0).unwrap();
    // let mutex = alloc.try_lock().unwrap();
    // assert_eq!(mutex.deref(), &0x01010101);
}
