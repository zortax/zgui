//! What one filled outline is told.

use bytemuck::{Pod, Zeroable};

/// One outline to fill, in the space of the pass region it belongs to.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Item {
    /// The quad, in the region's own pixels: origin then extent.
    pub bounds: [f32; 4],
    /// The scratch layer's extent, which is what maps the quad into clip space.
    ///
    /// The layer's and not the region's: a pass writes its region into the top-left of a layer that
    /// is usually larger, and what a draw is mapped onto is the whole layer.
    pub viewport: [f32; 4],
    /// Straight, gamma-encoded colour.
    pub color: [f32; 4],
    /// The first segment, how many, whether the rule is even-odd, and the first clip run.
    pub control: [f32; 4],
    /// How many clip runs, then three unused lanes.
    pub clips: [f32; 4],
}

/// Where one clip's outline is, as a triple the shader can read.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct Run {
    /// The first segment, how many, whether the rule is even-odd, then one unused lane.
    pub span: [f32; 4],
}

impl Run {
    /// A run of `count` segments starting at `first`, tested by the non-zero rule.
    pub fn new(first: usize, count: usize) -> Self {
        Self::of(first, count, false)
    }

    /// A run of `count` segments starting at `first`, tested by the named rule.
    ///
    /// A clip chain's links are regions with no rule of their own, so they take the non-zero one.
    /// A shape clip written in a vector document has whichever rule the document wrote, and a
    /// ring-shaped clip written even-odd is a ring under one rule and a disc under the other.
    pub fn of(first: usize, count: usize, even_odd: bool) -> Self {
        Self {
            span: [
                first as f32,
                count as f32,
                f32::from(u8::from(even_odd)),
                0.0,
            ],
        }
    }
}
