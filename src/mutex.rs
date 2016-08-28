use core;
use core::cell::UnsafeCell;
use core::mem;
use core::marker::PhantomData;
use core::ops::Deref;

use super::types::Result;
use super::pool::{RawPool, Block, Index, Full};

type TryLockResult<T> = core::result::Result<T, TryLockError>;

/// An enumeration of possible errors which can occur while calling the
/// `try_lock` method.
#[derive(Debug)]
pub enum TryLockError {
    /// The lock could not be acquired at this time because the operation would
    /// otherwise block.
    WouldBlock,
}

struct Pool<'a> {
    raw: UnsafeCell<RawPool<'a>>,
}

impl<'pool> Pool<'pool> {
    pub fn new(indexes:&'pool mut [Index], blocks: &'pool mut [Block]) -> Pool<'pool> {
        Pool{raw: UnsafeCell::new(RawPool::new(indexes, blocks))}
    }

    pub fn alloc<T>(&'pool self) -> Result<Mutex<'pool, T>> {
        let actual_size = mem::size_of::<T>() + mem::size_of::<Full>();
        let blocks = actual_size / mem::size_of::<Block>() +
            if actual_size % mem::size_of::<Block>() != 0 {1} else {0};
        unsafe {
            let pool = self.raw.get();
            let i = try!((*pool).alloc_index(blocks));
            Ok(Mutex{index: i, pool: self, _type: PhantomData})
        }
    }
}

struct Mutex<'a, T> {
    index: usize,
    pool: &'a Pool<'a>,
    _type: PhantomData<T>,
}

impl<'pool, T> Mutex<'pool, T> {
    pub fn try_lock(&'pool self) -> TryLockResult<MutexGuard<T>> {
        unsafe {
            let pool = self.pool.raw.get();
            let block = (*pool).index(self.index).block();
            let full = (*pool).full_mut(block);
            if full.is_locked() {
                Err(TryLockError::WouldBlock)
            } else {
                full.set_lock();
                assert!(full.is_locked());
                Ok(MutexGuard{__lock: self})
            }
        }
    }
}


struct MutexGuard<'a, T: 'a> {
    // Maybe remove this 'a?
    __lock: &'a Mutex<'a, T>,
}

impl<'mutex, T> Deref for MutexGuard<'mutex, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe {
            let pool = self.__lock.pool.raw.get();
            let index = &(*pool).index(self.__lock.index);
            mem::transmute((*pool).ptr(index.block()))
        }
    }
}

#[test]
fn it_works() {
    let mut indexes = [Index::default(); 256];
    let mut blocks = [Block::default(); 4096];
    let mut pool = Pool::new(&mut indexes[..], &mut blocks[..]);
    // let alloced = pool.alloc::<u32>();
    // let unwrapped_alloc = alloced.unwrap();
    // let locked = unwrapped_alloc.try_lock();
    // let unwrapped_locked = locked.unwrap();
    // assert_eq!(unwrapped_locked.deref(), &10);
}
