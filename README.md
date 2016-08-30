# defrag: fragmentation-free memory manager for microcontrollers

**This library is in the pre-Alpha stage and does not currently implement all
features. It is recommended NOT to use it at this time. Developers and comments
are always welcome.**

**defrag** is a minature memory manager that aims to eliminate the primary reason
NOT to use dynamic memory on microcontrollers -- that your memory might become
fragmented. **defrag** provides an ergonomic api for memory use modeled
after the standard mutex.

The primary manager of memory is the `Pool`, from which the user can call
`Pool.alloc::<T>()` or `Pool.alloc_slice::<T>(len)`. From this they will get
a `Mutex<T>` object which behaves very similarily to rust's stdlib
`Mutex` except it has only a single method, 
[`try_lock`](https://doc.rust-lang.org/std/sync/struct.Mutex.html#method.try_lock).
When the the user wishes to use the memory, they simply call `try_lock` to obtain
a reference to the underlying memory.

When the data is not locked, the underlying pool is allowed to move it in order
to solve potential fragmentation issues.

**defrag** utilizes several methods to enable memory reuse and avoid fragmentation
 - quick combination of free-memory blocks
 - an ultra-fast (O(8) max) binning method of memory reuse that favors speed over 
     trying to be "perfect" but still achieves good fragmentation reduction
 - as-needed step-wise defragmentation that does not cause long pauses in program
     execution

> This library is intended only for (single threaded) microcontrollers, so it's `Mutex`
> does not implement `Send` or `Sync` (it cannot be shared between threads). Depending
> on what kind of architectures or OS's spring up on uC rust code, this may change.

# Developers

Developer documentation can be found at DEVELOPER.md in this folder. Please reference
this for details on the internal workings of tinymem.

# Issues
If you find any bugs or have a feature requests, submit them to:

https://github.com/vitiral/defrag-rs/issues
