//! What a window's caches are allowed to hold, and the order they are asked to give it back in.
//!
//! Retained state without a budget is retained state that only grows. A window rasterises a glyph
//! once and keeps it, shapes a paragraph once and keeps it, places a drawing once and keeps it —
//! each of which is the right thing to do, and none of which has anything in it that says when to
//! stop. This is the one place that says.
//!
//! # What is here
//!
//! | Module | Contents |
//! |---|---|
//! | [`epoch`] | [`SceneEpoch`], the frame stamp a cache's last use is recorded against |
//! | [`report`] | [`CacheReport`] and [`BudgetReport`]: what is held, in what unit, and how far over |
//! | [`manager`] | [`Budgeted`], the registry, and the order eviction takes |
//! | [`limits`] | the levels the entry-counted caches are held to, and where the numbers come from |
//! | [`caches`] | the five adapters a window registers |
//!
//! # Why membership is not optional
//!
//! Two things need "every cache empty" to be a state a window can be put into, and both of them
//! break quietly rather than loudly if one cache can opt out. A window under memory pressure gets
//! nothing back from a cache that cannot be emptied. And a comparison between a window that has
//! been drawing for a while and one built from scratch is only a comparison if the first can be
//! made into the second — a cache left holding is a difference the comparison has stopped covering
//! while going on passing.
//!
//! So [`Budgeted::forget`] is a required method. There is no spelling of the trait that lets a
//! cache register for budgeting and not for being emptied, and the promise is asserted over the
//! registry rather than cache by cache, so a cache added later is covered the moment it is visited.
//!
//! # Units are carried, not assumed
//!
//! Three of the five caches are texture or texel memory and are budgeted in bytes. The other two
//! hold objects whose bulk is inside a shaper's or an allocator's own structures, where a byte
//! figure would be a guess wearing a measurement's clothes; those are budgeted in entries. Every
//! report says which, so nothing adds them together and no stated level needs a comment beside it
//! to be read.

pub mod caches;
pub mod epoch;
pub mod limits;
pub mod manager;
pub mod report;

pub use crate::budget::epoch::SceneEpoch;
pub use crate::budget::limits::CacheLimits;
pub use crate::budget::manager::{Budgeted, Budgets, CacheRegistry, Tracked, eviction_order};
pub use crate::budget::report::{BudgetReport, CacheId, CacheLine, CacheReport, CacheUnit};

#[cfg(test)]
mod tests;
