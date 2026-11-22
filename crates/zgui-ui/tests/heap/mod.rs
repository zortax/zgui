//! The live heap of the whole process, counted by the global allocator.

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicI64, Ordering};

/// Bytes allocated and not yet freed.
static LIVE: AtomicI64 = AtomicI64::new(0);

/// The system allocator, with every live byte counted.
struct Counting;

// SAFETY: every call forwards to the system allocator with the arguments it was given, and only
// adds the change in live bytes to a counter.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged; the caller upholds `alloc`'s contract.
        let block = unsafe { System.alloc(layout) };
        if !block.is_null() {
            LIVE.fetch_add(layout.size() as i64, Ordering::Relaxed);
        }
        block
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged; the caller upholds `alloc_zeroed`'s contract.
        let block = unsafe { System.alloc_zeroed(layout) };
        if !block.is_null() {
            LIVE.fetch_add(layout.size() as i64, Ordering::Relaxed);
        }
        block
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        // SAFETY: forwarded unchanged; the caller upholds `dealloc`'s contract.
        unsafe { System.dealloc(block, layout) };
        LIVE.fetch_sub(layout.size() as i64, Ordering::Relaxed);
    }

    unsafe fn realloc(&self, block: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: forwarded unchanged; the caller upholds `realloc`'s contract.
        let moved = unsafe { System.realloc(block, layout, size) };
        if !moved.is_null() {
            LIVE.fetch_add(size as i64 - layout.size() as i64, Ordering::Relaxed);
        }
        moved
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Returns the bytes allocated and not yet freed, over the whole process.
pub(crate) fn live() -> i64 {
    LIVE.load(Ordering::Relaxed)
}
