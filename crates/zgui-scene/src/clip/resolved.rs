//! The flattened clip a draw call binds.

use bytemuck::{Pod, Zeroable};
use zgui_atlas::AtlasTile;

/// One rounded-rectangle test, in the layout a shader reads it in.
///
/// The radii are eight floats — a horizontal and a vertical radius per corner, clockwise from the
/// top left — because CSS corners are elliptical and a scalar per corner cannot express
/// `border-radius: 80px / 20px`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct RoundedTest {
    /// The rectangle, as `[x, y, width, height]`.
    pub rect: [f32; 4],
    /// Elliptical radii, two per corner, clockwise from the top left.
    pub radii: [f32; 8],
}

/// A whole clip chain, flattened into what one draw call can apply.
///
/// A chain of any length collapses to an intersection rectangle plus at most two rounded tests plus
/// at most one coverage mask. A chain that needs more than that is not truncated — truncating a
/// clip is a silently wrong pixel — but promoted: the content is drawn into a target of its own and
/// composited, and [`ClipTable::needs_group_target`](crate::ClipTable::needs_group_target) is the
/// question that decides it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedClip {
    /// The intersection of every rectangle in the chain, as `[x, y, width, height]`.
    pub aabb: [f32; 4],
    /// Up to two rounded-corner tests; only the first `rounded_count` are meaningful.
    pub rounded: [RoundedTest; 2],
    /// How many of `rounded` are meaningful.
    pub rounded_count: u32,
    /// The coverage tile the chain samples, if it has one.
    pub mask: Option<AtlasTile>,
}

impl ResolvedClip {
    /// How far a chain that clips nothing extends, in device pixels.
    ///
    /// Finite rather than infinite so that intersecting with it is ordinary arithmetic: an infinite
    /// extent turns an empty intersection into a `NaN` and a clip test into a coin toss. Far larger
    /// than any surface, and far smaller than where `f32` loses integer precision.
    pub const UNBOUNDED: f32 = 1.0e7;

    /// The clip that admits everything.
    pub fn unbounded() -> Self {
        Self {
            aabb: [
                -Self::UNBOUNDED,
                -Self::UNBOUNDED,
                2.0 * Self::UNBOUNDED,
                2.0 * Self::UNBOUNDED,
            ],
            rounded: [RoundedTest::default(); 2],
            rounded_count: 0,
            mask: None,
        }
    }

    /// The intersection rectangle's left edge.
    pub fn left(&self) -> f32 {
        self.aabb[0]
    }

    /// The intersection rectangle's top edge.
    pub fn top(&self) -> f32 {
        self.aabb[1]
    }

    /// The intersection rectangle's right edge.
    pub fn right(&self) -> f32 {
        self.aabb[0] + self.aabb[2]
    }

    /// The intersection rectangle's bottom edge.
    pub fn bottom(&self) -> f32 {
        self.aabb[1] + self.aabb[3]
    }

    /// Whether the chain admits no pixels at all.
    ///
    /// Content under an empty clip is dropped before it reaches the display list, which is what
    /// makes a thousand-row table inside a scrollport cost its visible rows rather than all of
    /// them.
    pub fn is_empty(&self) -> bool {
        self.aabb[2] <= 0.0 || self.aabb[3] <= 0.0
    }
}
