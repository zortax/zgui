//! The fixed taxonomy of frame stages, and the tracing spans that name them.
//!
//! Every stage of producing a frame opens a span from this list and no other. A fixed taxonomy is
//! what makes two profiles comparable: a recording made a year apart, on a different machine, by
//! a different consumer of the trace, still divides the frame into the same named stages in the
//! same order, so "restyle got slower" is a statement a tool can make rather than a judgement a
//! person has to reconstruct.
//!
//! ```
//! use zgui_profile::Phase;
//!
//! let frame = zgui_profile::phase::frame(41);
//! let _frame = frame.enter();
//! let _stage = Phase::Layout.span().entered();
//! // …lay out…
//! ```

mod span;

#[cfg(test)]
mod tests;

pub use crate::phase::span::frame;

/// One stage of producing a frame, in the order the stages run.
///
/// The codes are the stable identity — [`Phase::Restyle`] is `P3` and will not become `P4` because
/// a stage was inserted before it — which is why stages that arrived between two existing ones
/// carry a fractional code rather than renumbering their neighbours.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Phase {
    /// Queued platform events are drained and dispatched to listeners.
    Events,
    /// Timers whose deadline has passed fire, in deadline order.
    Timers,
    /// The output device is re-examined: viewport size, scale factor, colour scheme.
    DeviceEpoch,
    /// Running transitions and animations advance to the current instant.
    Animate,
    /// The reactive graph is flushed, so every effect that a write invalidated re-runs.
    React,
    /// Selector matching and the cascade run over the elements that owe them.
    Restyle,
    /// What the restyle changed is translated into the work each node now owes.
    DamageTranslate,
    /// Text whose paint changed has its colour slot rewritten, so a cached text layout resolves
    /// to the new colour without being shaped again.
    TextPaint,
    /// The box tree is rebuilt where the element tree or its display types changed.
    BoxTree,
    /// Sizes and positions are computed.
    Layout,
    /// Absolute geometry, transforms, clips and stacking are resolved, and the result is compared
    /// against the previous frame's to find what moved or changed.
    Fragments,
    /// Size and position observations are delivered, and any resulting change settles.
    Observe,
    /// The pointer is hit-tested again where geometry moved underneath it.
    Rehit,
    /// The damaged region grows to cover everything that reads pixels outside what it writes.
    DamageExpand,
    /// Fragments intersecting the damaged region become scene primitives.
    Emit,
    /// The scene is finished: draw order assigned, primitives sorted and batched, passes planned.
    SceneFinish,
    /// The scene is drawn and the result presented.
    Render,
    /// The accessibility tree is updated, if anything is listening.
    A11y,
    /// Per-frame arenas are recycled and the loop decides how long to wait.
    Recycle,
}

impl Phase {
    /// Every stage, in the order they run.
    pub const ALL: [Phase; 19] = [
        Self::Events,
        Self::Timers,
        Self::DeviceEpoch,
        Self::Animate,
        Self::React,
        Self::Restyle,
        Self::DamageTranslate,
        Self::TextPaint,
        Self::BoxTree,
        Self::Layout,
        Self::Fragments,
        Self::Observe,
        Self::Rehit,
        Self::DamageExpand,
        Self::Emit,
        Self::SceneFinish,
        Self::Render,
        Self::A11y,
        Self::Recycle,
    ];

    /// The stage's stable code, such as `P3` or `P7.6`.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Events => "P0",
            Self::Timers => "P0.2",
            Self::DeviceEpoch => "P0.5",
            Self::Animate => "P1",
            Self::React => "P2",
            Self::Restyle => "P3",
            Self::DamageTranslate => "P4",
            Self::TextPaint => "P4.5",
            Self::BoxTree => "P5",
            Self::Layout => "P6",
            Self::Fragments => "P7",
            Self::Observe => "P7.6",
            Self::Rehit => "P7.5",
            Self::DamageExpand => "P8a",
            Self::Emit => "P8b",
            Self::SceneFinish => "P9",
            Self::Render => "P10",
            Self::A11y => "P11",
            Self::Recycle => "P12",
        }
    }

    /// The stage's name, as it appears in a trace.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Events => "events",
            Self::Timers => "timers",
            Self::DeviceEpoch => "device_epoch",
            Self::Animate => "animate",
            Self::React => "react",
            Self::Restyle => "restyle",
            Self::DamageTranslate => "damage_translate",
            Self::TextPaint => "text_paint",
            Self::BoxTree => "box_tree",
            Self::Layout => "layout",
            Self::Fragments => "fragments",
            Self::Observe => "observe",
            Self::Rehit => "rehit",
            Self::DamageExpand => "damage_expand",
            Self::Emit => "emit",
            Self::SceneFinish => "scene_finish",
            Self::Render => "render",
            Self::A11y => "a11y",
            Self::Recycle => "recycle",
        }
    }

    /// A span for this stage, to be entered for as long as the stage runs.
    ///
    /// Spans are opened at the `debug` level, so a consumer that only wants to see the shape of a
    /// frame gets it without also collecting everything a frame logs.
    pub fn span(self) -> tracing::Span {
        span::of(self)
    }
}

impl core::fmt::Display for Phase {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{} {}", self.code(), self.label())
    }
}
