//! Span construction for the frame stages.
//!
//! A span's name is part of its compile-time metadata, so it cannot be computed: each stage needs
//! its own literal. That is why this is a match over every variant rather than one call taking a
//! name, and it is also why the taxonomy has to be a closed set.

use tracing::debug_span;

use crate::phase::Phase;

/// The span for one stage.
pub(super) fn of(phase: Phase) -> tracing::Span {
    match phase {
        Phase::Events => debug_span!("events"),
        Phase::Timers => debug_span!("timers"),
        Phase::DeviceEpoch => debug_span!("device_epoch"),
        Phase::Animate => debug_span!("animate"),
        Phase::React => debug_span!("react"),
        Phase::Restyle => debug_span!("restyle"),
        Phase::DamageTranslate => debug_span!("damage_translate"),
        Phase::TextPaint => debug_span!("text_paint"),
        Phase::BoxTree => debug_span!("box_tree"),
        Phase::Layout => debug_span!("layout"),
        Phase::Fragments => debug_span!("fragments"),
        Phase::Observe => debug_span!("observe"),
        Phase::Rehit => debug_span!("rehit"),
        Phase::DamageExpand => debug_span!("damage_expand"),
        Phase::Emit => debug_span!("emit"),
        Phase::SceneFinish => debug_span!("scene_finish"),
        Phase::Render => debug_span!("render"),
        Phase::A11y => debug_span!("a11y"),
        Phase::Recycle => debug_span!("recycle"),
    }
}

/// The span covering a whole frame, which every stage span nests inside.
///
/// `index` is the frame's number, so that a trace can be cut at a frame boundary and two frames
/// can be told apart even when they did the same work.
///
/// ```
/// let frame = zgui_profile::phase::frame(0);
/// let _entered = frame.enter();
/// ```
pub fn frame(index: u64) -> tracing::Span {
    debug_span!("frame", index)
}
