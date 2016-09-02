/*!
This test framework intends to combine both performance and correctness tests
into a single test.

It will do this by measuring the performance of ONLY the allocation and deallocation,
not measure it's own checking

But performance checking will be secondary, the primary goal of this suite is
correctness testing.

It will have:
 - pseudo random number generator which ensures that allocations are identical
     on each run (can be altered by changing seed)
 - "Allocation Array" which tracks and determines which allocations shall be
      made.
*/

use core::mem;
use std::prelude::*;
use std::string::String;
use std::vec::Vec;
use core::iter::FromIterator;
use core::result;
use rand::{Rng, SeedableRng, XorShiftRng};
use super::*;

type Fill = u32;
type TResult<T> = result::Result<T, String>;


// struct Settings {
//     chance_deallocate: u8,
//     chance_change: u8,
//     chance_clean: u8,
//     max_len: u16,
// }

struct Allocation<'a> {
    i: usize,
    pool: &'a Pool,
    // settings: &'a Settings,
    data: Vec<Fill>,
    mutex: Option<super::SliceMutex<'a, Fill>>,
    // time: TIME,
}

impl<'a> Allocation<'a> {
    fn assert_valid(&self) -> TResult<()> {
        let mutex = match self.mutex {
            Some(ref m) => m,
            None => return Ok(()),
        };
        let ref sdata = self.data;
        let pdata = match mutex.try_lock() {
            Ok(l) => l,
            Err(e) => {
                println!("{}", self.pool.display());
                panic!("error trying lock: {:?}", e);
            }
        };

        if sdata.len() != pdata.len() {
            return Err(format!("lengths not equal: {} != {}", sdata.len(), pdata.len()));
        }
        for (i, (s, p)) in sdata.iter().zip(pdata.iter()).enumerate() {
            if s != p {
                return Err(format!("values at i={} differ: {} != {}", i, s, p));
            }
        }
        Ok(())
    }

    /// fill the Allocation up with data, don't check
    fn fill(&mut self, gen: &mut XorShiftRng) -> Result<()> {
        let mutex = match self.mutex {
            Some(ref m) => m,
            None => return Ok(()),
        };
        let sdata = &mut self.data;
        let mut pdata = mutex.try_lock().unwrap();
        for (i, (s, p)) in sdata.iter_mut().zip(pdata.iter_mut()).enumerate() {
            let val = gen.gen::<Fill>();
            *s = val;
            *p = val;
        }
        Ok(())
    }

    /// allocate some new data and fill it
    fn alloc(&mut self, gen: &mut XorShiftRng) -> TResult<()> {
        assert!(self.mutex.is_none());
        let len = gen.gen::<u16>() / 100;
        self.mutex = Some(match self.pool.alloc_slice::<Fill>(len) {
            Ok(m) => m,
            Err(e) => match e {
                Error::OutOfMemory => return Ok(()), // not allocated
                Error::Fragmented => {
                    // auto-defrag whenever it is encountered
                    self.pool.defrag();
                    match self.pool.alloc_slice::<Fill>(len) {
                        Ok(m) => m,
                        Err(e) => return Err(format!("alloc::alloc_slice2: {:?}", e)),
                    }
                }
                _ => return Err(format!("alloc::aloc_slice:{:?}", e)),
            }
        });
        self.data.clear();
        for _ in 0..len {
            self.data.push(0);
        }
        self.fill(gen);
        Ok(())
    }

    /// do something randomly
    fn do_random(&mut self, gen: &mut XorShiftRng) -> TResult<()> {
        try!(self.assert_valid());
        match self.mutex {
            // we have data, we need to decide what to do with it
            Some(_) => match gen.gen::<usize>() % 10 {
                0 => {
                    // deallocate the data
                    self.mutex = None;
                },
                1 ... 4 => {
                    // change the data
                    self.fill(gen);
                },
                8 ... 9 => {
                    self.pool.clean();
                },
                _ => {} // do nothing
            },
            // there is no data, should we allocate it?
            None => {
                match gen.gen::<usize>() % 10 {
                    0...3 => try!(self.alloc(gen)),
                    _ => {},
                }
            },
            // None => match gen.gen::<bool>() {}
        }
        try!(self.assert_valid());
        Ok(())
    }
}

#[test]
fn test_it() {
    // maxed out blocks, 1/10 max indexes
    let len_indexes = u16::max_value() / 10;
    let size = (u16::max_value() as usize * mem::size_of::<Block>()) / 4;
    let mut pool = Pool::new(size, len_indexes).expect("can't get pool");
    let mut allocs = Vec::from_iter(
        (0..len_indexes)
        .map(|i| Allocation {
            i: i as usize,
            pool: &pool,
            data: Vec::new(),
            mutex: None,
        }));
    let seed = [1, 2, 3, 4];
    let mut gen = XorShiftRng::from_seed(seed);
    println!("len allocs: {}", allocs.len());
    println!("some random values: {}, {}, {}",
             gen.gen::<u16>(), gen.gen::<u16>(), gen.gen::<u16>());
    for _ in 0..1000 {
        for alloc in allocs.iter_mut() {
            alloc.do_random(&mut gen).unwrap();
        }
    }
    println!("{}", pool.display());
}
