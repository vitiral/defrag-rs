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

use rand::{sample, Rng, SeedableRng, XorShiftRng};
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
    loops: usize,
    allocs: usize,
    frees: usize,
    cleans: usize,
    defrags: usize,
    out_of_mems: usize,
}

/// Actions that can be done when a full allocation is found
#[derive(Debug, Copy, Clone)]
enum FullActions {
    Deallocate,
    Clean,
    Change,
}

/// Actions that can be done when an empty allocation is found
#[derive(Debug, Copy, Clone)]
enum EmptyActions {
    Alloc,
    Skip,
}

#[derive(Debug, Default)]
struct Settings {
    /// the number of loops to run
    loops: usize,
    /// an action will randomly be selected from this list when
    /// data is found in an allocation
    full_chances: Vec<FullActions>,
    /// an action will randomly be selected from this list when
    /// data is not found in an allocation
    empty_chances: Vec<EmptyActions>,
}

/// contains means to track test as well as
/// settings for test
struct Tracker {
    gen: XorShiftRng,
    clock: Stopwatch,
    test_clock: Stopwatch,
    stats: Stats,
    settings: Settings,
}

impl Tracker {
    pub fn new(settings: Settings) -> Tracker {
        let seed = [1, 2, 3, 4];
        let mut gen = XorShiftRng::from_seed(seed);
        Tracker { gen: gen,
                  clock: Stopwatch::new(), test_clock: Stopwatch::new(),
                  stats: Stats::default(),
                  settings: settings,
        }
    }
}

struct Allocation<'a> {
    i: usize,
    pool: &'a Pool,
    data: Vec<Fill>,
    mutex: Option<super::SliceMutex<'a, Fill>>,
}

impl<'a> Allocation<'a> {
    fn assert_valid(&mut self) -> TResult<()> {
        let mutex = match self.mutex {
            Some(ref mut m) => m,
            None => return Ok(()),
        };
        let ref sdata = self.data;
        let pdata = mutex.lock();

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
            Some(ref mut m) => m,
            None => return Ok(()),
        };
        let edata = &mut self.data;
        let mut pdata = mutex.lock();
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
        let divider = self.pool.size() /
            (mem::size_of::<Fill>() * 64);
        let len = t.gen.gen::<u16>() % divider as u16;
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
            Some(_) => match sample(&mut t.gen, &t.settings.full_chances, 1)[0] {
                &FullActions::Deallocate => {
                    // deallocate the data
                    self.mutex = None;
                    t.stats.frees += 1;
                },
                &FullActions::Clean => {
                    // clean the data
                    t.clock.start();
                    self.pool.clean();
                    t.clock.stop();
                    t.stats.cleans += 1;
                },
                &FullActions::Change => {
                    // change the data
                    self.fill(t);
                },
            },
            // there is no data, should we allocate it?
            None => {
                match t.gen.gen::<usize>() % 100 {
                    0...50 => try!(self.alloc(t)),
                    _ => {},
                }
            },
        }
        try!(self.assert_valid());
        Ok(())
    }
}


// TODO: several parameters (like number of loops) need to be moved into settings
// and then several "benchmark" tests need to be created that can only be run in release
// mode... in release 1000 loops takes < 1 sec, in debug mode it takes over a minute.
fn do_test(pool: &Pool, allocs: &mut Vec<Allocation>, track: &mut Tracker) {
    println!("len allocs: {}", allocs.len());
    println!("some random values: {}, {}, {}",
             track.gen.gen::<u16>(), track.gen.gen::<u16>(), track.gen.gen::<u16>());
    track.test_clock.start();
    for _ in 0..track.settings.loops {
        for alloc in allocs.iter_mut() {
            alloc.do_random(track).unwrap();
        }
        track.stats.loops += 1;
    }
    track.test_clock.stop();
}

#[test]
fn test_it() {
    let blocks = u16::max_value() / 2;
    let size = blocks as usize * mem::size_of::<Block>();
    let len_indexes = blocks / 128;
    let mut pool = Pool::new(size, len_indexes).expect("can't get pool");
    let mut allocs = Vec::from_iter(
        (0..pool.len_indexes())
            .map(|i| Allocation {
                i: i as usize,
                pool: &pool,
                data: Vec::new(),
                mutex: None,
            }));
    // type FA = FullActions;
    // type EA = EmptyActions;
    let mut settings = Settings {
        loops: 50,
        full_chances: Vec::from_iter([FullActions::Deallocate; 9].iter().cloned()),
        empty_chances: vec![EmptyActions::Alloc],
    };
    settings.full_chances.push(FullActions::Clean);
    let mut track = Tracker::new(settings);
    let res = panic::catch_unwind(panic::AssertUnwindSafe(
        || do_test(&pool, &mut allocs, &mut track)));
    println!("{}", pool.display());
    println!("TIMES: test={}ms, pool={}ms",
             track.test_clock.elapsed_ms(),
             track.clock.elapsed_ms());
    println!("STATS: {:?}", track.stats);
    match res {
        Ok(_) => {},
        Err(e) => {
            panic::resume_unwind(e);
        }
    };
}
