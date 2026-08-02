//! Whether a mark may be written once per element rather than once per frame.
//!
//! A latency trace is a trace of *a frame*: a few dozen marks, one per stage boundary, whatever
//! the document is. A mark written per restyled element is a different measurement wearing the
//! same clothes, and it is the one thing a bounded ring cannot absorb — a restyle of a large
//! document writes more marks in one frame than the ring holds, so every frame boundary in it is
//! lost and the frame the trace was for cannot be found at all.
//!
//! Worse when something is *drawing* the trace: what it draws is elements, and elements are what
//! write the marks, so the reader feeds the writer and neither has a bound.
//!
//! So the per-element marks are their own decision, off unless somebody asks. Asking costs one
//! relaxed atomic load at each site, which is the same as the ring's own cost when nothing is
//! being kept.
//!
//! ```
//! assert!(!zgui_profile::latency::tracing_elements());
//! zgui_profile::latency::trace_elements(true);
//! assert!(zgui_profile::latency::tracing_elements());
//! # zgui_profile::latency::trace_elements(false);
//! ```

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the per-element marks are being written.
static TRACING: AtomicBool = AtomicBool::new(false);

/// The environment variable that turns them on for a whole run.
const ENVIRONMENT: &str = "ZGUI_LATENCY_ELEMENTS";

/// Starts or stops writing a mark per restyled element.
///
/// Independent of [`retain`](crate::latency::retain) and of
/// [`start_epoch`](crate::latency::start_epoch), and deliberately: retaining a ring is what an
/// inspector does to draw the shape of a frame, and it must not thereby turn on a per-element
/// trace whose volume is the document's size.
pub fn trace_elements(on: bool) {
    TRACING.store(on, Ordering::Relaxed);
}

/// Whether a mark per restyled element is wanted.
pub fn tracing_elements() -> bool {
    TRACING.load(Ordering::Relaxed)
}

/// Turns the per-element trace on when the environment asks for it.
///
/// Called once as the process's own trace file is opened, so that a run launched to produce a
/// trace can ask for the detailed one without an application being rebuilt to call
/// [`trace_elements`].
pub(super) fn read_environment() {
    if std::env::var_os(ENVIRONMENT).is_some() {
        trace_elements(true);
    }
}

#[cfg(test)]
mod tests {
    use super::{trace_elements, tracing_elements};

    #[test]
    fn the_switch_is_off_until_it_is_asked_for() {
        // Off is the whole point: the default has to be the one a linked-in inspector cannot turn
        // on by keeping a ring.
        assert!(!tracing_elements());
        trace_elements(true);
        assert!(tracing_elements());
        trace_elements(false);
        assert!(!tracing_elements());
    }
}
