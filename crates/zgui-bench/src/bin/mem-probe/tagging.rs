//! A global allocator that charges each allocation to the thread's allocation tag.
//!
//! Each block carries a 16-byte header in front of the bytes the caller gets: the tag and the
//! requested size. A block aligned to more than 16 bytes is padded so that the caller's bytes keep
//! their alignment, and the header sits in the last 16 bytes of the padding. A reallocated block
//! keeps the tag it was first charged to.

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

use zgui_profile::alloc_tag::{self, Tag};

/// The bytes in front of every block.
const HEADER: usize = 16;

/// What the header holds.
#[repr(C)]
struct Header {
    /// The tag's index.
    tag: u64,
    /// The size the caller asked for.
    size: u64,
}

const _: () = assert!(core::mem::size_of::<Header>() == HEADER);

/// The running figures for one tag.
struct Figures {
    /// Bytes allocated and not yet freed.
    live: AtomicU64,
    /// Blocks allocated and not yet freed.
    count: AtomicU64,
    /// The most bytes that were live at once.
    peak: AtomicU64,
    /// Blocks allocated since the start, reallocations excluded.
    allocs: AtomicU64,
    /// Bytes allocated since the start, growth by reallocation included.
    total: AtomicU64,
}

impl Figures {
    /// Figures that are all zero.
    const fn zero() -> Self {
        Self {
            live: AtomicU64::new(0),
            count: AtomicU64::new(0),
            peak: AtomicU64::new(0),
            allocs: AtomicU64::new(0),
            total: AtomicU64::new(0),
        }
    }

    /// Charges one new block of `size` bytes.
    fn add(&self, size: u64) {
        let live = self.live.fetch_add(size, Ordering::Relaxed) + size;
        self.peak.fetch_max(live, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.allocs.fetch_add(1, Ordering::Relaxed);
        self.total.fetch_add(size, Ordering::Relaxed);
    }

    /// Takes one freed block of `size` bytes off.
    fn remove(&self, size: u64) {
        self.live.fetch_sub(size, Ordering::Relaxed);
        self.count.fetch_sub(1, Ordering::Relaxed);
    }

    /// Moves one block from `old` to `new` bytes.
    fn resize(&self, old: u64, new: u64) {
        if new >= old {
            let grown = new - old;
            let live = self.live.fetch_add(grown, Ordering::Relaxed) + grown;
            self.peak.fetch_max(live, Ordering::Relaxed);
            self.total.fetch_add(grown, Ordering::Relaxed);
        } else {
            self.live.fetch_sub(old - new, Ordering::Relaxed);
        }
    }
}

/// The figures of every tag, by tag index.
static FIGURES: [Figures; Tag::COUNT] = [const { Figures::zero() }; Tag::COUNT];

/// One tag's figures at one moment.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TagFigures {
    /// Bytes live.
    pub(crate) live: u64,
    /// Blocks live.
    pub(crate) count: u64,
    /// The most bytes live at once since the start.
    pub(crate) peak: u64,
    /// Blocks allocated since the start.
    pub(crate) allocs: u64,
    /// Bytes allocated since the start.
    pub(crate) total: u64,
}

/// Every tag's figures, by tag index.
pub(crate) fn figures() -> [TagFigures; Tag::COUNT] {
    core::array::from_fn(|index| {
        let figures = &FIGURES[index];
        TagFigures {
            live: figures.live.load(Ordering::Relaxed),
            count: figures.count.load(Ordering::Relaxed),
            peak: figures.peak.load(Ordering::Relaxed),
            allocs: figures.allocs.load(Ordering::Relaxed),
            total: figures.total.load(Ordering::Relaxed),
        }
    })
}

/// The bytes the headers occupy right now, which the process holds and no tag is charged for.
pub(crate) fn header_bytes() -> u64 {
    FIGURES
        .iter()
        .map(|figures| figures.count.load(Ordering::Relaxed))
        .sum::<u64>()
        * HEADER as u64
}

/// How far the caller's bytes sit from the start of the block.
const fn offset(align: usize) -> usize {
    if align > HEADER { align } else { HEADER }
}

/// The block the system allocator is asked for, for a caller's `layout`.
fn outer(layout: Layout) -> Option<Layout> {
    let size = layout.size().checked_add(offset(layout.align()))?;
    Layout::from_size_align(size, layout.align().max(HEADER)).ok()
}

/// The tagging allocator.
pub(crate) struct Tagging;

impl Tagging {
    /// Writes the header in front of `user`, charges the block, and returns `user`.
    ///
    /// # Safety
    ///
    /// `base` is null or a block from [`outer`] of `layout`.
    unsafe fn finish(base: *mut u8, layout: Layout) -> *mut u8 {
        if base.is_null() {
            return base;
        }
        let tag = alloc_tag::current();
        // SAFETY: the block is `offset` bytes longer than the caller asked for, so the caller's
        // bytes start inside it.
        let user = unsafe { base.add(offset(layout.align())) };
        // SAFETY: the header is the 16 bytes in front of the caller's bytes, inside the block, and
        // 16-byte aligned because the block is and `offset` is a multiple of 16.
        unsafe {
            user.sub(HEADER).cast::<Header>().write(Header {
                tag: tag.index() as u64,
                size: layout.size() as u64,
            });
        }
        FIGURES[tag.index()].add(layout.size() as u64);
        user
    }

    /// The tag index a block's header holds.
    ///
    /// # Safety
    ///
    /// `user` was returned by this allocator and is not freed.
    unsafe fn tag_of(user: *mut u8) -> usize {
        // SAFETY: every block this allocator hands out has its header in the 16 bytes in front.
        let header = unsafe { user.sub(HEADER).cast::<Header>().read() };
        usize::try_from(header.tag).unwrap_or(0).min(Tag::COUNT - 1)
    }
}

// SAFETY: every block is a block of the system allocator, enlarged by the header and padding and
// given back with the same enlarged layout, so the system allocator's guarantees carry over.
unsafe impl GlobalAlloc for Tagging {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let Some(outer) = outer(layout) else {
            return core::ptr::null_mut();
        };
        // SAFETY: `outer` has a nonzero size, because it includes the header.
        let base = unsafe { System.alloc(outer) };
        // SAFETY: `base` is null or a block of `outer`.
        unsafe { Self::finish(base, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let Some(outer) = outer(layout) else {
            return core::ptr::null_mut();
        };
        // SAFETY: `outer` has a nonzero size, because it includes the header.
        let base = unsafe { System.alloc_zeroed(outer) };
        // SAFETY: `base` is null or a block of `outer`.
        unsafe { Self::finish(base, layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller hands back a live block of this allocator.
        let tag = unsafe { Self::tag_of(ptr) };
        FIGURES[tag].remove(layout.size() as u64);
        let outer = outer(layout).expect("a layout that was allocated has an enlarged layout");
        // SAFETY: the block starts `offset` bytes in front of the caller's bytes and was allocated
        // with `outer`.
        unsafe { System.dealloc(ptr.sub(offset(layout.align())), outer) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let start = offset(layout.align());
        let Some(new_outer) = new_size.checked_add(start) else {
            return core::ptr::null_mut();
        };
        // SAFETY: the caller hands back a live block of this allocator.
        let tag = unsafe { Self::tag_of(ptr) };
        let old_outer = outer(layout).expect("a layout that was allocated has an enlarged layout");
        // SAFETY: the block starts `start` bytes in front of the caller's bytes and was allocated
        // with `old_outer`; the new size keeps the same alignment.
        let base = unsafe { System.realloc(ptr.sub(start), old_outer, new_outer) };
        if base.is_null() {
            return base;
        }
        // SAFETY: the new block is `start` bytes longer than the caller asked for.
        let user = unsafe { base.add(start) };
        // SAFETY: the system allocator copied the header with the block; only the size changes.
        unsafe {
            user.sub(HEADER).cast::<Header>().write(Header {
                tag: tag as u64,
                size: new_size as u64,
            });
        }
        FIGURES[tag].resize(layout.size() as u64, new_size as u64);
        user
    }
}
