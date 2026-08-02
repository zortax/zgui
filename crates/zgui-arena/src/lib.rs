//! Storage whose addresses hold still, and the handles that name what it holds.
//!
//! Three ideas hold this crate together.
//!
//! **An address, once handed out, holds still.** [`ChunkArena`] stores values in blocks that are
//! allocated whole and never moved or resized, so a `&T` it hands out stays valid while unrelated
//! values are inserted, removed and read. A growable vector cannot promise that: it reallocates,
//! and a reallocation invalidates every reference into it at once. Anything that parcels
//! references out to worker threads and keeps building the structure they are walking needs the
//! former and is silently broken by the latter.
//!
//! **A handle is checked, not trusted.** [`Key`] packs a slot number, an occupancy counter and
//! the identity of the arena that minted it into eight bytes that are never all zero — so
//! [`Option`] of a key is eight bytes too. The counter moves on when a slot's value is dropped,
//! so a key to a value that is gone resolves to nothing rather than to whatever moved in
//! afterwards; the identity means a key from one document or one arena never resolves inside
//! another. A slot that exhausts its counters is retired rather than risk the distinction
//! collapsing.
//!
//! **Removal is deferred by a frame.** [`ChunkArena::remove`] marks a value dead but leaves it in
//! place, and [`ChunkArena::recycle`] — called once at the end of a frame, after every pass has
//! run — drops it and offers its slot back. So a value removed part-way through a frame is still
//! readable through its key for the rest of that frame, and a slot number never comes to mean
//! something different part-way through one. Passes that hold keys across each other therefore
//! need no coordination beyond running inside the same frame.
//!
//! # Side tables
//!
//! Data that only some values carry lives beside the arena rather than inside it, in a table
//! indexed by the same key. [`SlotVec`] is dense — one entry per slot, for data nearly everything
//! has. [`PagedVec`] is sparse — a page of a thousand entries allocated on first write, for data
//! most things do not have, where a dense table would be mostly empty and much larger than
//! everything it describes put together.
//!
//! ```
//! use zgui_arena::{ChunkArena, DomainId, PagedVec, SlotVec};
//!
//! let mut arena: ChunkArena<&str> = ChunkArena::new(DomainId::FIRST);
//! let mut depth: SlotVec<_, u32> = SlotVec::for_domain(arena.domain());
//! let mut label: PagedVec<_, Option<String>> = PagedVec::for_domain(arena.domain());
//!
//! let root = arena.insert("root");
//! let child = arena.insert("child");
//! depth.insert(root, 0);
//! depth.insert(child, 1);
//! *label.get_mut(root) = Some("the root".to_owned());
//!
//! assert_eq!(arena.get(child), Some(&"child"));
//! assert_eq!(depth.get(child), Some(&1));
//! assert_eq!(label.pages(), 1, "one page covers a thousand slots, labelled or not");
//!
//! arena.remove(child);
//! assert_eq!(arena.get(child), Some(&"child"), "the frame is not over");
//! arena.recycle();
//! assert_eq!(arena.get(child), None);
//! ```
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`key`] | [`Key`], [`Generation`], [`DomainId`] and the [`ArenaKey`] trait |
//! | [`chunk`] | [`ChunkArena`] and its block storage |
//! | [`slot_vec`] | [`SlotVec`], the dense side table |
//! | [`paged_vec`] | [`PagedVec`], the sparse side table |

#![deny(missing_docs)]
// Handing out a reference that survives an unrelated insertion is the one thing this crate needs
// `unsafe` for. Every raw-memory operation lives in `chunk::block`, where each one states what
// its caller must uphold; only the two modules that discharge those obligations against the slot
// bookkeeping may call them, and no public function in the crate is `unsafe`.
#![deny(unsafe_code)]

pub mod chunk;
pub mod key;
pub mod paged_vec;
pub mod slot_vec;

#[cfg(test)]
mod proptest_config;

pub use crate::chunk::{BLOCK_LEN, ChunkArena};
pub use crate::key::{
    ARENA_KIND_COUNT, ArenaKey, ArenaKind, DOCUMENT_COUNT, DocumentId, DomainId, Generation, Key,
};
pub use crate::paged_vec::{PAGE_LEN, PagedVec};
pub use crate::slot_vec::SlotVec;
