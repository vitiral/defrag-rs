use super::types::*;

use super::pool::{RawPool, Block, Index, Full};
use core::mem;
use core::cell::UnsafeCell;
use core::marker::PhantomData;

use core::ops::{Deref, DerefMut};

pub struct Pool<'a> {
    raw: UnsafeCell<RawPool<'a>>,
}

pub struct Mutex<'a, T> {
    index: index,
    pool: &'a Pool<'a>,
    data: PhantomData<T>,
}

// impl<'pool, T: Sized> Mutex<'pool, T> {
//     fn new_unsized(pool: &Pool, size: usize) -> Result<Mutex<'pool, T>> {
//     }
// }

pub struct MutexGuard<'a, T: 'a> {
    __lock: &'a Mutex<'a, T>,
}

/// A type alias for the result of a nonblocking locking method.
pub type TryLockResult<Guard> = core::result::Result<Guard, TryLockError>;

/// An enumeration of possible errors which can occur while calling the
/// `try_lock` method.
#[derive(Debug)]
pub enum TryLockError {
    /// The lock could not be acquired at this time because the operation would
    /// otherwise block.
    WouldBlock,
}


impl <'pool> Pool<'pool> {
    pub fn new(indexes:&'pool mut [Index], blocks: &'pool mut [Block]) -> Pool<'pool> {
        Pool {
            raw: UnsafeCell::new(RawPool::new(indexes, blocks)),
        }
    }

    pub fn alloc<T>(&'pool self) -> Result<Mutex<'pool, T>> {
        let actual_size = mem::size_of::<T>() + mem::size_of::<Full>();
        // get ceil(actual_size, sizeof(Block))
        let blocks = actual_size / mem::size_of::<Block>() +
            if actual_size % mem::size_of::<Block>() != 0 {1} else {0};
        unsafe {
            let pool = self.raw.get();
            let i = try!((*pool).alloc_index(blocks));
            Ok(Mutex {
                index: i,
                pool: self,
                data: PhantomData,
            })
        }
    }
}


impl<'pool, T: Sized> Mutex<'pool, T> {
    /// Attempts to acquire this lock.
    ///
    /// If the lock could not be acquired at this time, then `Err` is returned.
    /// Otherwise, a system-dependent guard is returned. The lock will be unlocked when the
    /// guard is dropped.
    ///
    /// This function does not block.
    ///
    /// # Errors
    ///
    /// If another user of this mutex panicked while holding the mutex, then
    /// this call will return failure if the mutex would otherwise be
    /// acquired.
    pub fn try_lock(&'pool self) -> TryLockResult<MutexGuard<T>> {
        unsafe {
            let pool = self.pool.raw.get();
            let index = &mut (*pool).indexes[self.index];
            let full = index.full_mut(&mut (*pool));
            if full.is_locked() {
                Err(TryLockError::WouldBlock)
            } else {
                full.set_lock();
                Ok(MutexGuard::new(self))
            }
        }
    }
}

impl<'a, T: Sized> Drop for Mutex<'a, T> {
    fn drop(&mut self) {
        unsafe {
            (*self.pool.raw.get()).dealloc_index(self.index);
        }
    }
}

impl<'mutex, T: Sized> MutexGuard<'mutex, T> {
    unsafe fn new(lock: &'mutex Mutex<'mutex, T>) -> MutexGuard<'mutex, T> {
        MutexGuard {
            __lock: lock,
        }
    }
}

impl<'mutex, T: Sized> Deref for MutexGuard<'mutex, T> {
    type Target = T;

    fn deref(&self) -> &T {
        let i = self.__lock.index;
        unsafe {
            let pool = self.__lock.pool.raw.get();
            let index = (*pool).indexes[i];
            mem::transmute(index.ptr(&*pool))
        }
    }
}

impl<'mutex, T: Sized> DerefMut for MutexGuard<'mutex, T> {
    fn deref_mut(&mut self) -> &mut T {
        let i = self.__lock.index;
        unsafe {
            let pool = self.__lock.pool.raw.get();
            let index = (*pool).indexes[i];
            mem::transmute(index.ptr(&mut *pool))
        }
    }
}
#[test]
fn test_alloc() {
    // test using raw indexes
    let mut indexes = [Index::default(); 256];
    let mut blocks = [Block::default(); 4096];
    let mut pool = Pool::new(&mut indexes, &mut blocks);

    let aval = pool.alloc::<i32>();
    let uval = aval.unwrap();
    let rval = uval.try_lock();
    // let ref_val = val.try_lock();
    // assert_eq!(&10, i.try_lock().unwrap().deref());
}
