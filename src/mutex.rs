use super::types::*;

use super::pool::{RawPool, Block, Index};

pub struct Pool<'a> {
    raw: core::cell::UnsafeCell<RawPool<'a>>,
}

pub struct Mutex<'a, T: ?Sized> {
    index: index,
    pool: &'a Pool<'a>,
    data: core::cell::UnsafeCell<T>,
}

pub struct MutexGuard<'a, T: ?Sized + 'a> {
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


impl<'pool, T: ?Sized> Mutex<'pool, T> {
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
            let full = index.full(&mut (*pool));
            if full.is_locked() {
                Err(TryLockError::WouldBlock)
            } else {
                full.set_lock();
                Ok(MutexGuard::new(self))
            }
        }
    }
}

impl<'a, T: ?Sized> Drop for Mutex<'a, T> {
    fn drop(&mut self) {
        unsafe {
            (*self.pool.raw.get()).dealloc_index(self.index);
        }
    }
}

impl<'mutex, T: ?Sized> MutexGuard<'mutex, T> {
    unsafe fn new(lock: &'mutex Mutex<'mutex, T>) -> MutexGuard<'mutex, T> {
        MutexGuard {
            __lock: lock,
        }
    }
}
// impl<'mutex, T: ?Sized> MutexGuard<'mutex, T> {
//     unsafe fn new(lock: &'mutex Mutex<T>) -> MutexGuard<'mutex, T> {
//         MutexGuard {
//             __lock: lock,
//         }
//     }
// }
