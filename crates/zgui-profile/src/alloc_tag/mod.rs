//! Allocation tags: the part of the pipeline that an allocation on this thread belongs to.
//!
//! A tagging global allocator reads [`current`] for each allocation and charges the bytes to that
//! tag. The frame sets the tag at each of its stage marks (see [`phase_of_mark`]). A few sites set
//! a narrower tag for the duration of a call with [`scope`].
//!
//! The tag exists only with the `alloc-tags` feature. Without the feature, [`scope`] calls its
//! body, [`enter`] returns an empty guard and [`current`] returns [`Tag::Other`]. A call site then
//! compiles to the same code as no call.
//!
//! ```
//! use zgui_profile::alloc_tag::{self, Tag};
//!
//! let tag = alloc_tag::scope(Tag::Shaping, alloc_tag::current);
//! if alloc_tag::TAGS_ENABLED {
//!     assert_eq!(tag, Tag::Shaping);
//! } else {
//!     assert_eq!(tag, Tag::Other);
//! }
//! assert_eq!(alloc_tag::current(), Tag::Other);
//! ```

#[cfg(test)]
mod tests;

use crate::Phase;

/// The part of the pipeline an allocation is charged to.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(u8)]
pub enum Tag {
    /// No stage claims the allocation.
    Other,
    /// The application's own data, outside the view.
    AppModel,
    /// Building a view outside the reactive flush.
    ReactiveBuild,
    /// The reactive flush: effects that run again.
    ReactiveRun,
    /// Changes to the document's nodes.
    Dom,
    /// Selector matching, the cascade and what the restyle changed.
    Style,
    /// The box tree.
    BoxTree,
    /// The layout engine and the inline layout it calls.
    LayoutEngine,
    /// Text shaping.
    Shaping,
    /// Line breaking of shaped text.
    Break,
    /// Fragments, geometry observations and what they start.
    Fragments,
    /// The hit-test index.
    Hit,
    /// Paint records: damage expansion and the emit walk.
    PaintRecord,
    /// Scene assembly: draw order, sorting and batching.
    Scene,
    /// The renderer.
    Renderer,
    /// The accessibility tree.
    A11y,
}

impl Tag {
    /// How many tags there are.
    pub const COUNT: usize = 16;

    /// Every tag, in index order.
    pub const ALL: [Tag; Self::COUNT] = [
        Self::Other,
        Self::AppModel,
        Self::ReactiveBuild,
        Self::ReactiveRun,
        Self::Dom,
        Self::Style,
        Self::BoxTree,
        Self::LayoutEngine,
        Self::Shaping,
        Self::Break,
        Self::Fragments,
        Self::Hit,
        Self::PaintRecord,
        Self::Scene,
        Self::Renderer,
        Self::A11y,
    ];

    /// The tag's position in [`Tag::ALL`].
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The tag at `index`, or [`Tag::Other`] for an index past the last tag.
    pub const fn from_index(index: usize) -> Self {
        if index < Self::COUNT {
            Self::ALL[index]
        } else {
            Self::Other
        }
    }

    /// The tag's name in a report.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Other => "other",
            Self::AppModel => "app_model",
            Self::ReactiveBuild => "reactive_build",
            Self::ReactiveRun => "reactive_run",
            Self::Dom => "dom",
            Self::Style => "style",
            Self::BoxTree => "box_tree",
            Self::LayoutEngine => "layout_engine",
            Self::Shaping => "shaping",
            Self::Break => "break",
            Self::Fragments => "fragments",
            Self::Hit => "hit",
            Self::PaintRecord => "paint_record",
            Self::Scene => "scene",
            Self::Renderer => "renderer",
            Self::A11y => "a11y",
        }
    }
}

impl Phase {
    /// The tag that allocations made during this stage are charged to.
    pub const fn alloc_tag(self) -> Tag {
        match self {
            Self::Events | Self::Timers | Self::DeviceEpoch | Self::Animate | Self::Recycle => {
                Tag::Other
            }
            Self::React => Tag::ReactiveRun,
            Self::Restyle | Self::DamageTranslate | Self::TextPaint => Tag::Style,
            Self::BoxTree => Tag::BoxTree,
            Self::Layout => Tag::LayoutEngine,
            Self::Fragments | Self::Observe => Tag::Fragments,
            Self::Rehit => Tag::Hit,
            Self::DamageExpand | Self::Emit => Tag::PaintRecord,
            Self::SceneFinish => Tag::Scene,
            Self::Render => Tag::Renderer,
            Self::A11y => Tag::A11y,
        }
    }
}

/// Whether this build keeps a tag at all.
pub const TAGS_ENABLED: bool = cfg!(feature = "alloc-tags");

#[cfg(feature = "alloc-tags")]
thread_local! {
    /// This thread's tag, as an index into [`Tag::ALL`].
    ///
    /// Constant-initialised and without a destructor, so an allocator can read it at any time,
    /// also while the thread ends.
    static CURRENT: core::cell::Cell<u8> = const { core::cell::Cell::new(0) };
}

/// Replaces this thread's tag and returns the previous one, as indices.
#[cfg(feature = "alloc-tags")]
#[inline]
fn replace(tag: u8) -> u8 {
    CURRENT
        .try_with(|current| current.replace(tag))
        .unwrap_or(0)
}

/// This thread's tag.
///
/// Always [`Tag::Other`] without the `alloc-tags` feature.
#[inline(always)]
pub fn current() -> Tag {
    #[cfg(feature = "alloc-tags")]
    {
        Tag::from_index(usize::from(
            CURRENT.try_with(core::cell::Cell::get).unwrap_or(0),
        ))
    }
    #[cfg(not(feature = "alloc-tags"))]
    {
        Tag::Other
    }
}

/// Sets this thread's tag until the guard drops, and then puts the previous tag back.
///
/// Without the `alloc-tags` feature the guard is empty and has no destructor.
#[must_use = "the tag goes back when this drops"]
pub struct Scope {
    /// The tag to put back, as an index.
    #[cfg(feature = "alloc-tags")]
    previous: u8,
}

#[cfg(feature = "alloc-tags")]
impl Drop for Scope {
    #[inline]
    fn drop(&mut self) {
        replace(self.previous);
    }
}

/// Sets this thread's tag to `tag` until the returned guard drops.
#[inline(always)]
pub fn enter(tag: Tag) -> Scope {
    #[cfg(feature = "alloc-tags")]
    {
        let previous = replace(tag as u8);
        Scope { previous }
    }
    #[cfg(not(feature = "alloc-tags"))]
    {
        let _ = tag;
        Scope {}
    }
}

/// Runs `body` with this thread's tag set to `tag`, and then puts the previous tag back.
///
/// The previous tag also goes back when `body` unwinds.
#[inline(always)]
pub fn scope<R>(tag: Tag, body: impl FnOnce() -> R) -> R {
    let _scope = enter(tag);
    body()
}

/// The stage that a frame's latency mark begins, for the marks the frame loop writes.
///
/// A mark outside this table, such as a damage note, a resize event or a note about work that
/// is complete (`f.restyled`, `b.why`, `t.reshaped`, `w.laidout`), returns `None`, and a tag that
/// follows the marks stays as it is.
pub fn phase_of_mark(stage: &str) -> Option<Phase> {
    Some(match stage {
        "f.begin" | "f.drain" => Phase::Events,
        "f.timers" => Phase::Timers,
        "f.reconfigure" | "f.device" => Phase::DeviceEpoch,
        "f.scroll" | "f.gestures" | "f.raf" | "f.animate" => Phase::Animate,
        "f.flush" | "f.commands" | "f.images" => Phase::React,
        "f.restyle" => Phase::Restyle,
        "f.brushes" => Phase::TextPaint,
        "f.boxes" => Phase::BoxTree,
        "f.layout" => Phase::Layout,
        "f.fragments" | "f.image_demand" | "f.enter" | "f.dispatch_scroll" | "f.images_late"
        | "f.publish_brushes" | "f.caret" | "f.embed" => Phase::Fragments,
        "f.observe" => Phase::Observe,
        "f.rehit" => Phase::Rehit,
        "f.paint" | "p.expand" => Phase::DamageExpand,
        "p.emit" => Phase::Emit,
        "p.finish" | "p.budget" => Phase::SceneFinish,
        "p.upload" | "p.draw" | "draw.in" | "draw.undamaged" | "r.tables" | "r.vectors"
        | "r.plan" | "r.encoder" | "r.buffers" | "r.record" | "r.shift" | "acq.in" | "acq.out"
        | "sub.out" | "pres.out" => Phase::Render,
        "f.a11y" => Phase::A11y,
        "f.painted" | "f.recycle" | "f.end" => Phase::Recycle,
        _ => return None,
    })
}

/// Sets this thread's tag from a latency mark, if the mark begins a stage.
///
/// The tag stays until the next such mark, so each stage of the frame is charged for what it
/// allocates. A [`scope`] that is open while a mark sets the tag puts its own previous tag back
/// when it ends.
#[cfg(feature = "alloc-tags")]
pub(crate) fn follow_mark(stage: &str) {
    if let Some(phase) = phase_of_mark(stage) {
        replace(phase.alloc_tag() as u8);
    }
}
