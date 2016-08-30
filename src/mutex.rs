use core;
use core::cell::UnsafeCell;
use core::mem;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

use super::types::{Result, Error, index, block};
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

/// return the ceiling of a / b
fn ceil(a: usize, b: usize) -> usize {
    a / b + (if a % b != 0 {1} else {0})
}

impl Pool {
    pub fn new(raw: *mut RawPool) -> Pool {
        Pool { raw: raw }
    }

    pub fn alloc<T>(&self) -> Result<Mutex<T>> {
        unsafe {
            let actual_size: usize = mem::size_of::<Full>() + mem::size_of::<T>();
            let blocks = ceil(actual_size, mem::size_of::<Block>());
            if blocks > (*self.raw).len_blocks() as usize {
                return Err(Error::InvalidSize);
            }
            let i = try!((*self.raw).alloc_index(blocks as u16));
            Ok(Mutex{index: i, pool: self, _type: PhantomData})
        }
    }

    pub fn alloc_slice<T>(&self, len: block) -> Result<SliceMutex<T>> {
        unsafe {
            let actual_size: usize = mem::size_of::<Full>() + mem::size_of::<T>() * len as usize;
            let blocks = ceil(actual_size, mem::size_of::<Block>());
            if blocks > (*self.raw).len_blocks() as usize {
                return Err(Error::InvalidSize);
            }
            let i = try!((*self.raw).alloc_index(blocks as u16));
            Ok(SliceMutex{index: i, len: len, pool: self, _type: PhantomData})
        }
    }
}

// ##################################################
// # Standard Mutex

struct Mutex<'a, T> {
    index: index,
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

// ##################################################
// # Slice Mutex

struct SliceMutex<'a, T> {
    index: index,
    pool: &'a Pool,
    len: block,
    _type: PhantomData<T>,
}

impl<'a, T> SliceMutex<'a, T> {
    pub fn try_lock(&'a self) -> TryLockResult<SliceMutexGuard<T>> {
        unsafe {
            let pool = &*self.pool.raw;
            let block = pool.index(self.index).block();
            let full = pool.full_mut(block);
            if full.is_locked() {
                Err(TryLockError::WouldBlock)
            } else {
                full.set_lock();
                assert!(full.is_locked());
                Ok(SliceMutexGuard{__lock: self})
            }
        }
    }
}

struct SliceMutexGuard<'a, T: 'a> {
    __lock: &'a SliceMutex<'a, T>,
}

impl<'a, T: 'a> SliceMutexGuard<'a, T> {
    fn deref(&mut self) -> &[T] {
        unsafe {
            let pool = &*self.__lock.pool.raw;
            let index = &pool.index(self.__lock.index);
            let t: *const T = mem::transmute(pool.ptr(index.block()));
            core::slice::from_raw_parts(t, self.__lock.len as usize)
        }
    }

    fn deref_mut(&mut self) -> &mut [T] {
        unsafe {
            let pool = &*self.__lock.pool.raw;
            let index = &pool.index(self.__lock.index);
            let t: *mut T = mem::transmute(pool.ptr(index.block()));
            core::slice::from_raw_parts_mut(t, self.__lock.len as usize)
        }
    }
}

#[test]
fn test_alloc() {
    let mut indexes = [Index::default(); 256];
    let mut blocks = [Block::default(); 4096];
    let mut raw_pool = unsafe {
        let iptr: *mut Index = unsafe { mem::transmute(&mut indexes[..][0]) };
        let bptr: *mut Block = unsafe { mem::transmute(&mut blocks[..][0]) };
        RawPool::new(iptr, indexes.len() as index, bptr, blocks.len() as block)
    };

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

fn test_alloc_slice() {
    let mut indexes = [Index::default(); 256];
    let mut blocks = [Block::default(); 4096];
    let mut raw_pool = unsafe {
        let iptr: *mut Index = unsafe { mem::transmute(&mut indexes[..][0]) };
        let bptr: *mut Block = unsafe { mem::transmute(&mut blocks[..][0]) };
        RawPool::new(iptr, indexes.len() as index, bptr, blocks.len() as block)
    };

    let praw = &mut raw_pool as *mut RawPool;
    let mut pool = Pool::new(praw);

    {
        let alloced = pool.alloc_slice::<u16>(10000);
        let unwrapped_alloc = alloced.unwrap();
        let locked = unwrapped_alloc.try_lock();
        let mut unwrapped_locked = locked.unwrap();
        {
            let rmut = unwrapped_locked.deref_mut();
            for n in 0..10000 {
                assert_eq!(rmut[n], 0);
                rmut[n] = n as u16;
            }
        }

        let r = unwrapped_locked.deref_mut();
        for n in 0..10000 {
            assert_eq!(r[n], n as u16);
        }
    }
}
