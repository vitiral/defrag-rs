use super::types::*;

use super::pool::{RawPool, Block, Index, Full};
use core::mem;

use core::ops::{Deref, DerefMut};

pub struct Pool<'a> {
    raw: core::cell::UnsafeCell<RawPool<'a>>,
}

pub struct Mutex<'a, T: Sized + 'a> {
    index: index,
    pool: &'a Pool<'a>,
    data: core::marker::PhantomData<&'a T>,
}

// impl<'pool, T: Sized> Mutex<'pool, T> {
//     fn new_unsized(pool: &Pool, size: usize) -> Result<Mutex<'pool, T>> {
//     }
// }

pub struct MutexGuard<'a, T: Sized + 'a> {
    __lock: &'a Mutex<'a, T>,
}

/// A type alias for the result of a nonblocking locking method.
pub type TryLockResult<Guard> = core::result::Result<Guard, TryLockError>;

/// An enumeration of possible errors which can occur while calling the
/// `try_lock` method.
pub enum TryLockError {
    /// The lock could not be acquired at this time because the operation would
    /// otherwise block.
    WouldBlock,
}

impl <'pool> Pool<'pool> {
    pub fn alloc<T>(&'pool self) -> Result<Mutex<'pool, T>>
    {
        self.alloc_unsized(mem::size_of::<T>())
    }

    pub fn alloc_unsized<T: Sized>(&'pool self, size: usize) -> Result<Mutex<'pool, T>> {
        let actual_size = size + mem::size_of::<Full>();
        // get ceil(actual_size, sizeof(Block))
        let blocks = actual_size / mem::size_of::<Block>() +
            if actual_size % mem::size_of::<Block>() != 0 {1} else {0};
        unsafe {
            let pool = self.raw.get();
            let i = try!((*pool).alloc_index(blocks));
            Ok(Mutex {
                index: i,
                pool: self,
                data: core::marker::PhantomData,
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

// impl<'mutex, T: Sized> DerefMut for MutexGuard<'mutex, T> {
//     fn deref_mut(&mut self) -> &mut T {
//         let i = self.__lock.index;
//         unsafe {
//             let pool = self.__lock.pool.raw.get();
//             mem::tran*(*pool).indexes[i].ptr(&*pool)
//         }
//     }
// }
