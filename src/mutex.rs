use core;
use core::cell::UnsafeCell;
use core::mem;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

use super::types::{Result, index, block};
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

struct Pool {
    raw: *mut RawPool,
}

impl Pool {
    pub fn new(raw: *mut RawPool) -> Pool {
        Pool { raw: raw }
    }

    pub fn alloc<T>(&self) -> Result<Mutex<T>> {
        let actual_size = mem::size_of::<T>() + mem::size_of::<Full>();
        let blocks = actual_size / mem::size_of::<Block>() +
            if actual_size % mem::size_of::<Block>() != 0 {1} else {0};
        unsafe {
            let i = try!((*self.raw).alloc_index(blocks));
            Ok(Mutex{index: i, pool: self, _type: PhantomData})
        }
    }
}

struct Mutex<'a, T> {
    index: usize,
    pool: &'a Pool,
    _type: PhantomData<T>,
}

impl<'a, T> Mutex<'a, T> {
    pub fn try_lock(&'a self) -> TryLockResult<MutexGuard<T>> {
        unsafe {
            let pool = &*self.pool.raw;
            let block = pool.index(self.index).block();
            let full = pool.full_mut(block);
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

impl<'a, T: 'a> Deref for MutexGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe {
            let pool = &*self.__lock.pool.raw;
            let index = &pool.index(self.__lock.index);
            mem::transmute(pool.ptr(index.block()))
        }
    }
}

impl<'a, T: 'a> DerefMut for MutexGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe {
            let pool = &*self.__lock.pool.raw;
            let index = &pool.index(self.__lock.index);
            mem::transmute(pool.ptr(index.block()))
        }
    }
}

#[test]
fn it_works() {
    let mut indexes = [Index::default(); 256];
    let mut blocks = [Block::default(); 4096];
    let len_i = indexes.len();
    let iptr: *mut Index = unsafe { mem::transmute(&mut indexes[..][0]) };
    let len_b = blocks.len();
    let bptr: *mut Block = unsafe { mem::transmute(&mut blocks[..][0]) };
    let mut raw_pool = RawPool::new(iptr, len_i, bptr, len_b);

    let praw = &mut raw_pool as *mut RawPool;
    let mut pool = Pool::new(praw);

    let expected = 0x01010101;

    let alloced = pool.alloc::<u32>();
    let unwrapped_alloc = alloced.unwrap();
    let locked = unwrapped_alloc.try_lock();
    let mut unwrapped_locked = locked.unwrap();
    {
        let rmut = unwrapped_locked.deref_mut();
        *rmut = expected;
    }
    assert_eq!(unwrapped_locked.deref(), &expected);

    let expected2 = -1000;
    let alloced2 = pool.alloc::<i64>();
    let unwrapped_alloc2 = alloced2.unwrap();
    let locked2 = unwrapped_alloc2.try_lock();
    let mut unwrapped_locked2 = locked2.unwrap();
    {
        let rmut = unwrapped_locked2.deref_mut();
        *rmut = expected2;
    }
    assert_eq!(unwrapped_locked2.deref(), &expected2);

    println!("{:?}, {:?}", indexes[0].block(), blocks[0].dumb());
}
