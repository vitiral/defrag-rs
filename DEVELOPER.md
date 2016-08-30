# defrag Developer Documentation

## Table of Contents
- [Basic Theory](#basic-theory)
- [Defragmentation](#defragmentation)
- [Memory Reuse](#memory-reuse)


## Basic Theory

The first thing to understand is that **defrag** is created to be simple. The goal 
is to get a defragmenting memory manager working and worry about performance 
later. Despite this fact, most of the features of **defrag** are already designed 
to work relatively fast. This is necessary, as you could easily spend billions 
of CPU cycles defragmenting memory with a poorly designed system. For this 
reason, the initial implementation of **defrag** carefully balances simplicity with 
speed, opting for the simpler solution when it is “fast enough.” (at least for 
now)

Before going into the inner workings of **defrag**, let’s first go over the goals 
of a mico-memory manager (in order of importance):

- easy to use
- small and lightweight
    - able to handle at least 64k of managed memory (for modern ARM processors)
- reliable and robust over long term usage
    - no lost memory
    - able to defragment memory
    - able to handle calls to obtain large chunks of memory

One of the biggest challenges to creating a memory manager (without an 
operating system) is that pointers point to a place in memory, so therefore
we can’t move any data to defragment it. For instance, let’s say I have the 
following:

```
# TOTAL MEMORY
 A       B   C
|X X X X|X X|X X X X X|
```

If I free block **B** it will look like:
```
# TOTAL MEMORY
 A       B   C
|X X X X|- -|X X X X X|
```

**Note:**
> These kind of charts will be used frequently. |X X X| represents full memory
> and |- - -| represents freed memory. **A, B and C** are pointers to memory
> locations

There are three options:
 1. Make sure to use B when another call requires that much memory (or less)
 2. Move some later chunk of memory (let’s call it D) that is less than or equal 
      to B into B’s current location.
 3. Move C backwards so the memory is no longer fragmented

The first option has the problem that the application might never want that much 
memory again, or if there are enough “holes” like B, it won’t have enough memory
to grant a request for a large chunk.

What we would like to do is option 3 which is to move an already existing 
chunk of memory backwards, eliminating B entirely. However, doing so would screw 
up any pointers that are pointing at the original chunk of memory, as they would 
be invalid. For instance, if I move C, backwards, a pointer that thinks it is 
pointing to the beginning of C is now pointing somewhere in the middle!

If only we could move C without breaking the user’s program, then it should be 
relatively simple to defragment memory... 

This is the first thing you have to understand when trying to understand 
**defrag**. **defrag** reduces all of the issues with memory defragmentation into a 
single problem: I can’t move memory in the heap when I want to. 

To solve this problem, **defrag** simply circumvents it. It allows you to move 
block C backwards by putting an application layer between the user’s code and 
access to pointer C. 

## Defragmentation

Being able to fully defragment memory is the primary feature of **defrag** that
differentiates it from other memory managers. The current implementation does
it very simply. 

From our example above, say we have memory that looks like this:

```
# TOTAL MEMORY
 A       B   C         D         heap
|X X X X|- -|X X X X X|X X X X X|...
```

The full defragmentation does what everybody wants to do, it simply moves **C**
and **D** backwards, eliminating **B**

```
# TOTAL MEMORY
 A       C         D         more heap than before
|X X X X|X X X X X|X X X X X|...
```

## Storing Freed Values

One of the most significant problems for u-memory managers is storing
which values have been freed.

To do this, **defrag** uses a trick: all allocated values MUST be at least
4 `block` indexes in size. Then, when a value is freed the information to
construct a linked-list and keep track of size and location are stored in
the empty memory itself

So, heaps looks something like this:
```
 A         B       C       D       E
|X X X X X|0 D - -|X X X X|B E - -|D 0 - - - - -|
              \           /  \    /
               -----------    ----
```

In this way all freed memory is kept track of without using any "real"
data.

## Memory Reuse

> Most of this code is in `src/tm_freed.c`

The other primary requirement for a memory manager is re-using memory
that has previously been freed as efficiently as possible. If I allocate 
a 4byte array, and then free it, I don’t want to use heap memory if 
I allocate another 4byte array

There are several `Pool` data structures that are used to store freed
values

To do this, the start of the freed-linked lists are kept in sized bins.
There are 8 bins of size N Block:
 - >= 2^0  = 1 blocks (must be >= `sizeof(Free)`)
 - >= 2^2  = 4 blocks
 - >= 2^4  = 16 blocks
 - >= 2^6  = 64 blocks
 - >= 2^8  = 256 blocks
 - >= 2^10 = 1024 blocks
 - >= 2^12 = 4096 blocks
 - larger than 4096

when an allocation is selected, it knows it can get the value from the 
bin which is >= it's size in blocks.
