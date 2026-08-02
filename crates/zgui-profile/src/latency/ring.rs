//! Keeping the last few thousand marks in memory, for a reader inside the process.
//!
//! The file the rest of [`latency`](crate::latency) writes is for a reader *outside* the process: a
//! run finishes, somebody reads the trace and finds out where the time went. A tool that draws the
//! shape of the frame that has just been painted cannot use it — it would be reading a file it is
//! itself still writing, one flush behind, with no way to tell which marks belong to the frame on
//! screen.
//!
//! So the marks can also be kept here, in a bounded ring: the last `capacity` of them, oldest
//! dropped, readable at any moment as a snapshot. Nothing is kept unless somebody asks for it with
//! [`retain`], and a mark costs one relaxed atomic load when nobody has.
//!
//! ```
//! zgui_profile::latency::retain(64);
//! zgui_profile::latency::mark("f.begin");
//! zgui_profile::latency::note("f.end", "presented");
//!
//! let recent = zgui_profile::latency::recent();
//! assert_eq!(recent.len(), 2);
//! assert_eq!(recent[1].stage, "f.end");
//! assert_eq!(recent[1].note, "presented");
//! assert!(recent[1].at_ns >= recent[0].at_ns);
//! # zgui_profile::latency::retain(0);
//! ```

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// One mark, as a reader inside the process sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recorded {
    /// What happened.
    pub stage: &'static str,
    /// What it had to say.
    pub note: String,
    /// When, in nanoseconds since the first mark this ring kept.
    pub at_ns: u128,
}

/// The ring, once one has been asked for.
struct Ring {
    /// The zero every mark is relative to.
    base: Instant,
    /// How many marks are kept.
    capacity: usize,
    /// What is kept.
    marks: Mutex<VecDeque<Recorded>>,
}

/// Whether anything is being kept, read on every mark.
static KEEPING: AtomicBool = AtomicBool::new(false);

/// The ring itself.
static RING: OnceLock<Ring> = OnceLock::new();

/// Starts keeping the last `capacity` marks in memory, or stops when `capacity` is zero.
///
/// The capacity is fixed by the first call that asks for a non-zero one: a ring that could be
/// resized would have to reallocate under a lock every mark takes, and the point of the bound is
/// that the cost of recording does not depend on how long the process has been running.
pub fn retain(capacity: usize) {
    if capacity == 0 {
        KEEPING.store(false, Ordering::Relaxed);
        return;
    }
    RING.get_or_init(|| Ring {
        base: Instant::now(),
        capacity,
        marks: Mutex::new(VecDeque::with_capacity(capacity)),
    });
    KEEPING.store(true, Ordering::Relaxed);
}

/// Whether marks are being kept in memory.
pub fn retaining() -> bool {
    KEEPING.load(Ordering::Relaxed)
}

/// Every mark still in the ring, oldest first.
pub fn recent() -> Vec<Recorded> {
    let Some(ring) = RING.get() else {
        return Vec::new();
    };
    ring.marks
        .lock()
        .map(|marks| marks.iter().cloned().collect())
        .unwrap_or_default()
}

/// The last `count` marks in the ring, oldest first.
///
/// What a reader inside the process wants, and [`recent`] is what it must not use: the ring holds
/// hundreds of frames so that a reader is never handed a half-written one, and every mark in it
/// carries a heap-allocated note, so copying the whole thing to look at one frame's worth costs a
/// few thousand allocations per read. This copies what was asked for and nothing else.
///
/// A `count` larger than the ring holds returns everything in it, which is the same answer
/// [`recent`] gives.
pub fn last(count: usize) -> Vec<Recorded> {
    let Some(ring) = RING.get() else {
        return Vec::new();
    };
    ring.marks
        .lock()
        .map(|marks| {
            let skip = marks.len().saturating_sub(count);
            marks.iter().skip(skip).cloned().collect()
        })
        .unwrap_or_default()
}

/// Empties the ring, so the next read sees only what happened after this.
pub fn clear() {
    let Some(ring) = RING.get() else {
        return;
    };
    if let Ok(mut marks) = ring.marks.lock() {
        marks.clear();
    }
}

/// Adds a mark to the ring, dropping the oldest when it is full.
pub(super) fn push(stage: &'static str, note: &str, at: Instant) {
    if !KEEPING.load(Ordering::Relaxed) {
        return;
    }
    let Some(ring) = RING.get() else {
        return;
    };
    let at_ns = at.saturating_duration_since(ring.base).as_nanos();
    if let Ok(mut marks) = ring.marks.lock() {
        if marks.len() == ring.capacity {
            marks.pop_front();
        }
        marks.push_back(Recorded {
            stage,
            note: note.to_owned(),
            at_ns,
        });
    }
}
