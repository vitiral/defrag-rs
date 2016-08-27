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

/// A type alias for the result of a nonblocking locking method.
pub type TryLockResult<T> = core::result::Result<T, TryLockError>;

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
    pub fn try_lock(&self) -> TryLockResult<&'pool mut T> {
        unsafe {
            let pool = self.pool.raw.get();
            let index = &mut (*pool).indexes[self.index];
            let full = index.full_mut(&mut (*pool));
            if full.is_locked() {
                Err(TryLockError::WouldBlock)
            } else {
                full.set_lock();
                let pool = self.pool.raw.get();
                let index = (*pool).indexes[self.index];
                let out: &'pool mut T = mem::transmute(index.ptr(&mut *pool));
                Ok(out)
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

#[test]
fn test_alloc() {
    // test using raw indexes
    let mut indexes = [Index::default(); 256];
    let mut blocks = [Block::default(); 4096];
    let mut pool = Pool::new(&mut indexes, &mut blocks);

    let val = pool.alloc::<i32>().unwrap();
    let ref_val = val.try_lock().unwrap();
    assert_eq!(&10, ref_val);
    // let ref_val = val.try_lock();
    // assert_eq!(&10, i.try_lock().unwrap().deref());
}
