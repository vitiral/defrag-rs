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

use std::thread;
use std::prelude::*;
use std::string::String;
use std::vec::Vec;
use std::panic;

use core::mem;
use core::iter::FromIterator;
use core::result;

use rand::{Rng, SeedableRng, XorShiftRng};
use stopwatch::Stopwatch;

use super::*;

type Fill = u32;
type TResult<T> = result::Result<T, String>;

impl panic::UnwindSafe for Pool {}


// struct Settings {
//     chance_deallocate: u8,
//     chance_change: u8,
//     chance_clean: u8,
//     max_len: u16,
// }

#[derive(Debug, Default)]
struct Stats {
    allocs: usize,
    frees: usize,
    cleans: usize,
    defrags: usize,
    out_of_mems: usize,
}

/// contains means to track test as well as
/// settings for test
struct Tracker {
    gen: XorShiftRng,
    clock: Stopwatch,
    test_clock: Stopwatch,
    stats: Stats,
}

impl Tracker {
    pub fn new() -> Tracker {
        let seed = [1, 2, 3, 4];
        let mut gen = XorShiftRng::from_seed(seed);
        Tracker { gen: gen,
                  clock: Stopwatch::new(), test_clock: Stopwatch::new(),
                  stats: Stats::default() }
    }
}

struct Allocation<'a> {
    i: usize,
    pool: &'a Pool,
    // settings: &'a Tracker,
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
    fn fill(&mut self, t: &mut Tracker) -> Result<()> {
        let mutex = match self.mutex {
            Some(ref m) => m,
            None => return Ok(()),
        };
        let edata = &mut self.data;
        let mut pdata = mutex.try_lock().unwrap();
        for (i, (e, p)) in edata.iter_mut().zip(pdata.iter_mut()).enumerate() {
            let val = t.gen.gen::<Fill>();
            *e = val;
            *p = val;
        }
        Ok(())
    }

    /// allocate some new data and fill it
    fn alloc(&mut self, t: &mut Tracker) -> TResult<()> {
        assert!(self.mutex.is_none());
        let len = t.gen.gen::<u16>() / 100;
        t.clock.start();
        let slice = self.pool.alloc_slice::<Fill>(len);
        t.clock.stop();
        self.mutex = Some(match slice {
            Ok(m) => {
                t.stats.allocs += 1;
                m
            },
            Err(e) => match e {
                Error::OutOfMemory => {
                    t.stats.out_of_mems += 1;
                    return Ok(()); // not allocated
                },
                Error::Fragmented => {
                    // auto-defrag whenever it is encountered
                    t.clock.start();
                    self.pool.defrag();
                    let slice = self.pool.alloc_slice::<Fill>(len);
                    t.clock.stop();
                    t.stats.defrags += 1;
                    match slice {
                        Ok(m) => {
                            t.stats.allocs += 1;
                            m
                        },
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
        self.fill(t);
        Ok(())
    }

    /// do something randomly
    fn do_random(&mut self, t: &mut Tracker) -> TResult<()> {
        try!(self.assert_valid());
        match self.mutex {
            // we have data, we need to decide what to do with it
            Some(_) => match t.gen.gen::<usize>() % 100 {
                0...50 => {
                    // deallocate the data
                    self.mutex = None;
                    t.stats.frees += 1;
                },
                51...60 => {
                    // clean the data
                    t.clock.start();
                    self.pool.clean();
                    t.clock.stop();
                    t.stats.cleans += 1;
                },
                _ => {
                    // change the data
                    self.fill(t);
                },
            },
            // there is no data, should we allocate it?
            None => {
                match t.gen.gen::<usize>() % 90 {
                    0...3 => try!(self.alloc(t)),
                    _ => {},
                }
            },
        }
        try!(self.assert_valid());
        Ok(())
    }
}


fn do_test(pool: &mut Pool) -> Tracker {
    let mut allocs = Vec::from_iter(
        (0..pool.len_indexes())
            .map(|i| Allocation {
                i: i as usize,
                pool: &pool,
                data: Vec::new(),
                mutex: None,
            }));
    println!("len allocs: {}", allocs.len());
    let mut track = Tracker::new();
    println!("some random values: {}, {}, {}",
             track.gen.gen::<u16>(), track.gen.gen::<u16>(), track.gen.gen::<u16>());
    track.test_clock.start();
    for _ in 0..1000 {
        for alloc in allocs.iter_mut() {
            alloc.do_random(&mut track).unwrap();
        }
    }
    track.test_clock.stop();
    track
}

#[test]
fn test_it() {
    let len_indexes = u16::max_value() / 10;
    let size = (u16::max_value() as usize * mem::size_of::<Block>()) / 4;
    let mut pool = Pool::new(size, len_indexes).expect("can't get pool");
    let res = panic::catch_unwind(panic::AssertUnwindSafe(|| do_test(&mut pool)));
    let track = match res {
        Ok(t) => t,
        Err(e) => {
            println!("{}", pool.display());
            thread::sleep_ms(200);
            panic::resume_unwind(e);
        }
    };
    // maxed out blocks, 1/10 max indexes
    println!("{}", pool.display());
    println!("TIMES: test={}ms, pool={}ms",
             track.test_clock.elapsed_ms(),
             track.clock.elapsed_ms());
    println!("STATS: {:?}", track.stats);

}
