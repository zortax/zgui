//! The animation and interaction properties nothing has claimed yet.
//!
//! Every row here parses and cascades: an author may write it and the value reaches the
//! computed style. What none of them has is a reader — and that is not asserted, it is measured
//! twice over. Each one has a static probe that sets it on a fixture, and each also goes through
//! the timed harness, which runs the animation stage itself over a series of moments. A row that
//! starts moving something under either instrument fails.
//!
//! The rest of the motion vocabulary is not here: the thirteen longhands the engine's own animation
//! driver reads are declared beside that driver, in `zgui-style::parity`.

use crate::parity::support::Support;

/// Why the scroll-driven half has no reader.
///
/// The whole family belongs to scroll-driven animations, which the engine parses and its animation
/// driver never consults. Nothing above the engine can supply what is missing, so the note names
/// the engine rather than this framework's plans.
const SCROLL_DRIVEN: &str = "the engine's animation driver does not read it: scroll-driven \
                             animations are parsed and never sampled";

crate::register_properties! {
    animation_composition => Support::Ignored(SCROLL_DRIVEN),
    animation_range_end => Support::Ignored(SCROLL_DRIVEN),
    animation_range_start => Support::Ignored(SCROLL_DRIVEN),
    animation_timeline => Support::Ignored(SCROLL_DRIVEN),
}
