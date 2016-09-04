use core::result;
use core::default::Default;
use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;
use core::mem;
use core::slice;

use alloc::heap;

use super::types::{Result, Error, IndexLoc, BlockLoc};
use super::raw_pool::{RawPool, Index, Block, Full, DisplayPool};

pub type TryLockResult<T> = result::Result<T, TryLockError>;

/// An enumeration of possible errors which can occur while calling the
/// `try_lock` method.
#[derive(Debug)]
pub enum TryLockError {
    /// The lock could not be acquired at this time because the operation would
    /// otherwise block.
    WouldBlock,
}
/// return the ceiling of a / b
fn ceil(a: usize, b: usize) -> usize {
    a / b + (if a % b != 0 {1} else {0})
}

/**
`Pool` contains a "pool" of memory which can be allocated and used

Pool memory can be accessed through the `alloc` and `alloc_slice` methods
returning memory protected behind a `Mutex`. The Mutex allows Pool to
defragment application memory when it is not in use, solving the problem
of memory fragmentation for embedded systems.
*/
pub struct Pool {
    raw: *mut RawPool,
}

impl Drop for Pool {
    fn drop(&mut self) {
        unsafe {
            let align = mem::size_of::<usize>();
            let size_raw = mem::size_of::<RawPool>();
            let raw = &mut *self.raw;
            let size_indexes = raw.len_indexes() as usize * mem::size_of::<Index>();
            let size_blocks = raw.len_blocks() as usize * mem::size_of::<Block>();

            // They have to be deallocated in the order they were allocated
            heap::deallocate(raw._indexes as *mut u8, size_indexes, align);
            heap::deallocate(raw._blocks as *mut u8, size_blocks, align);
            heap::deallocate(self.raw as *mut u8, size_raw, align);
        }
    }
}

impl Pool {
    /**
    Create a new pool of the requested size and number of indexes

    `size` is total size of the internal block pool. Some of this space
    will be used to keep track of the size and index of the allocated data.

    `indexes` are the total number of indexes available. This is the maximum
    number of simultanious allocations that can be taken out of the pool.
    Allocating a Mutex uses an index, dropping the Mutex frees the index.
    */
    pub fn new(size: usize, indexes: IndexLoc) -> Result<Pool> {
        let num_blocks = ceil(size, mem::size_of::<Block>());
        if indexes > IndexLoc::max_value() / 2
                || num_blocks > BlockLoc::max_value() as usize / 2 {
            return Err(Error::InvalidSize)
        }
        unsafe {
            let num_indexes = indexes;
            let size_raw = mem::size_of::<RawPool>();
            // allocate our memory
            let align = mem::size_of::<usize>();
            let pool = heap::allocate(size_raw, align);
            if pool.is_null() {
                return Err(Error::OutOfMemory);
            }
            let size_indexes = indexes as usize * mem::size_of::<Index>();
            let indexes = heap::allocate(size_indexes, align);
            if indexes.is_null() {
                heap::deallocate(pool, size_raw, align);
                return Err(Error::OutOfMemory);
            }
            let size_blocks = num_blocks * mem::size_of::<Block>();
            let blocks = heap::allocate(size_blocks, align);
            if blocks.is_null() {
                heap::deallocate(indexes, size_indexes, align);
                heap::deallocate(pool, size_raw, align);
                return Err(Error::OutOfMemory);
            }

            let pool = pool as *mut RawPool;
            let indexes = indexes as *mut Index;
            let blocks = blocks as *mut Block;

            // initialize our memory and return
            *pool = RawPool::new(indexes, num_indexes, blocks, num_blocks as u16);
            Ok(Pool::from_raw(pool))
        }
    }

    /// get a Pool from a RawPool that you have initialized
    ///
    /// it is important that you call mem::forget on the Pool
    /// and deallocate the underlying memory yourself
    /// when you are done with it
    pub unsafe fn from_raw(raw: *mut RawPool) -> Pool {
        Pool { raw: raw }
    }

    /// attempt to allocate memory of type T, returning `Result<Mutex<T>>`
    ///
    /// If `Ok(Mutex<T>)`, the memory will have been initialized to `T.default`
    /// and can be unlocked and used by calling `Mutex.try_lock`
    ///
    /// For error results, see `Error`
    pub fn alloc<T: Default>(&self) -> Result<Mutex<T>> {
        unsafe {
            let actual_size: usize = mem::size_of::<Full>() + mem::size_of::<T>();
            let blocks = ceil(actual_size, mem::size_of::<Block>());
            if blocks > (*self.raw).len_blocks() as usize {
                return Err(Error::InvalidSize);
            }
            let i = try!((*self.raw).alloc_index(blocks as u16));
            let index = (*self.raw).index(i);
            let mut p = (*self.raw).data(index.block()) as *mut T;
            *p = T::default();
            Ok(Mutex{index: i, pool: self, _type: PhantomData})
        }
    }

    /// attempt to allocate a slice of memory with `len` of `T` elements,
    /// returning `Result<SliceMutex<T>>`
    ///
    /// If `Ok(SliceMutex<T>)`, all elements of the slice will have been initialized to `T.default`
    /// and can be unlocked and used by calling `Mutex.try_lock`
    ///
    /// For error results, see `Error`
    pub fn alloc_slice<T: Default>(&self, len: BlockLoc) -> Result<SliceMutex<T>> {
        unsafe {
            let actual_size: usize = mem::size_of::<Full>() + mem::size_of::<T>() * len as usize;
            let blocks = ceil(actual_size, mem::size_of::<Block>());
            if blocks > (*self.raw).len_blocks() as usize {
                return Err(Error::InvalidSize);
            }
            let i = try!((*self.raw).alloc_index(blocks as u16));
            let index = (*self.raw).index(i);
            let mut p = (*self.raw).data(index.block()) as *mut T;
            for _ in 0..len {
                *p = T::default();
                p = p.offset(1);
            }
            Ok(SliceMutex{index: i, len: len, pool: self, _type: PhantomData})
        }
    }


    /// call this to be able to printout the status
    /// of the `Pool`
    pub fn display(&self) -> DisplayPool {
        unsafe {
            (*self.raw).display()
        }
    }

    /// clean the `Pool`, combining contigous blocks
    /// of free memory
    pub fn clean(&self) {
        unsafe { (*self.raw).clean() }
    }

    /// defragment the `Pool`, combining blocks of
    /// used memory and increasing the size of the
    /// heap
    pub fn defrag(&self) {
        unsafe { (*self.raw).defrag() }
    }

    /// get the total size of the `Pool` in bytes
    pub fn size(&self) -> usize {
        unsafe { (*self.raw).size() }
    }

    /// get the total number of indexes in the `Pool`
    pub fn len_indexes(&self) -> IndexLoc {
        unsafe { (*self.raw).len_indexes() }
    }
}

// ##################################################
// # Standard Mutex

/**
all allocated data is represented as a Mutex. When the data
is unlocked, the underlying `Pool` is free to move it and
reduce fragmentation

See https://doc.rust-lang.org/std/sync/struct.Mutex.html for
more information on the general API
*/
pub struct Mutex<'a, T> {
    index: IndexLoc,
    pool: &'a Pool,
    _type: PhantomData<T>,
}

impl<'mutex, T> Mutex<'mutex, T> {
    /**
    get a usable value, locking the underlying memory from being
    used

    While the memory is locked, it cannot be moved which means
    that defragmentation is not as efficient as possible.
    It is recommended to `drop` the returned `Value` as soon
    as possible (i.e. let it go out of scope)

    Note that currently, Mutex can only exist in a single thread
    which means that `lock` is always non-blocking.
    */
    pub fn lock<'a>(&'a mut self) -> Value<'a, 'mutex, T> {
        unsafe {
            let pool = &*self.pool.raw;
            let block = pool.index(self.index).block();
            let full = pool.full_mut(block);
            assert!(!full.is_locked());
            full.set_lock();
            assert!(full.is_locked());
            Value {__lock: self}
        }
    }
}

impl<'a, T> Drop for Mutex<'a, T> {
    fn drop(&mut self) {
        unsafe {
            (*self.pool.raw).dealloc_index(self.index)
        }
    }
}


/**
A value which can be used through `Deref`

When this is dropped, the memory it is referencing
is automatically unlocked, which allows it to be
defragmented. This object should be dropped as
soon as possible to allow for defragmentation.
*/
pub struct Value<'a, 'mutex: 'a, T: 'mutex> {
    __lock: &'a Mutex<'mutex, T>,
}

impl<'a, 'mutex: 'a, T: 'mutex> Drop for Value<'a, 'mutex, T> {
    fn drop(&mut self) {
        unsafe {
            let pool = &mut *self.__lock.pool.raw;
            let index = pool.index(self.__lock.index);
            pool.full_mut(index.block()).clear_lock();
        }
    }
}

impl<'a, 'mutex: 'a, T: 'mutex> Deref for Value<'a, 'mutex, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe {
            let pool = &*self.__lock.pool.raw;
            let index = &pool.index(self.__lock.index);
            &*(pool.data(index.block()) as *const T)
        }
    }
}

impl<'a, 'mutex: 'a, T: 'a> DerefMut for Value<'a, 'mutex, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe {
            let pool = &*self.__lock.pool.raw;
            let index = &pool.index(self.__lock.index);
            &mut *(pool.data(index.block()) as *mut T)
        }
    }
}

// ##################################################
// # Slice Mutex

/// same as `Mutex` except wrapps a `Slice`
pub struct SliceMutex<'a, T> {
    index: IndexLoc,
    pool: &'a Pool,
    len: BlockLoc,
    _type: PhantomData<T>,
}

impl<'a, T> Drop for SliceMutex<'a, T> {
    fn drop(&mut self) {
        unsafe {
            (*self.pool.raw).dealloc_index(self.index)
        }
    }
}

impl<'mutex, T> SliceMutex<'mutex, T> {
    /**
    get a usable slice, locking the underlying memory from being
    used

    See `Mutex.lock`
     */
    pub fn lock<'a>(&'a mut self) -> Slice<'a, 'mutex, T> {
        unsafe {
            let pool = &*self.pool.raw;
            let block = pool.index(self.index).block();
            let full = pool.full_mut(block);
            assert!(!full.is_locked());
            full.set_lock();
            assert!(full.is_locked());
            Slice {__lock: self}
        }
    }
}

/**
A [`slice`](https://doc.rust-lang.org/std/slice) which
can be used through `Deref`

See `Value`
*/
pub struct Slice<'a, 'mutex: 'a, T: 'mutex> {
    __lock: &'a mut SliceMutex<'mutex, T>,
}

impl<'a, 'mutex: 'a, T: 'mutex> Drop for Slice<'a, 'mutex, T> {
    fn drop(&mut self) {
        unsafe {
            let pool = &mut *self.__lock.pool.raw;
            let index = pool.index(self.__lock.index);
            pool.full_mut(index.block()).clear_lock();
        }
    }
}

impl<'a, 'mutex: 'a, T: 'mutex> Deref for Slice<'a, 'mutex, T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        unsafe {
            let pool = &*self.__lock.pool.raw;
            let index = &pool.index(self.__lock.index);
            let t: *const T = mem::transmute(pool.data(index.block()));
            // slice::from_raw_parts::<'a>(t, self.__lock.len as usize)
            slice::from_raw_parts(t, self.__lock.len as usize)
        }
    }
}


impl<'a, 'mutex: 'a, T: 'a> DerefMut for Slice<'a, 'mutex, T> {
    fn deref_mut(&mut self) -> &mut [T] {
        unsafe {
            let pool = &*self.__lock.pool.raw;
            let index = &pool.index(self.__lock.index);
            let t: *mut T = mem::transmute(pool.data(index.block()));
            slice::from_raw_parts_mut(t, self.__lock.len as usize)
        }
    }
}

#[test]
fn test_alloc() {
    let pool = Pool::new(4096, 256).unwrap();
    let expected = 0x01010101;

    let mut mutex = pool.alloc::<u32>().unwrap();
    let mut locked = mutex.lock();

    {
        let rmut = locked.deref_mut();
        *rmut = expected;
    }
    assert_eq!(locked.deref(), &expected);

    let expected2 = -1000;
    let mut mutex2 = pool.alloc::<i64>().unwrap();
    let mut locked2 = mutex2.lock();
    {
        let rmut = locked2.deref_mut();
        *rmut = expected2;
    }
    assert_eq!(locked2.deref(), &expected2);
}

#[test]
fn test_alloc_slice() {
    let pool = Pool::new(4096 * mem::size_of::<Block>(), 256).unwrap();

    let mut mutex = pool.alloc_slice::<u16>(10000).unwrap();
    let mut slice = mutex.lock();
    // TODO: make a test that makes sure this doesn't compile
    // let mut slice2 = mutex.lock();
    {
        let rmut = slice.deref_mut();
        for n in 0..10000 {
            assert_eq!(rmut[n], 0);
            rmut[n] = n as u16;
        }
    }

    {
        let r = slice.deref_mut();
        for n in 0..10000 {
            assert_eq!(r[n], n as u16);
        }
    }
}
