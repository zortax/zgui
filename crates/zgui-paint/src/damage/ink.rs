//! Two rectangles per fragment: what it paints, and what it reads.
//!
//! Nearly every fragment reads exactly what it paints, and the whole design leans on that: the few
//! that do not are registered while the fragment tree is built, so the damage expansion is a walk
//! over a handful of entries rather than over the document.
//!
//! The two callers of [`read_extent_of`] — the expansion that runs before anything is emitted, and
//! the cull that decides whether a fragment survives — are deliberately the same function. They have
//! to agree about which pixels a composite reads, and two implementations of one question is how two
//! callers come to disagree about it.

use zgui_geom::{Device, DevicePx, Rect};
use zgui_layout::fragment::filter;
use zgui_layout::{FragKey, Fragment, LayoutStore};
use zgui_scene::read_extent;

/// Which target a composite reads outside what it writes.
///
/// The distinction decides how far the damage has to grow for it. A content filter reads the
/// composite's *own* target, which the pool lends fresh every frame — so every pixel it reads has
/// to be painted this frame, and the damage grows to the whole read region. A backdrop reads what
/// is beneath it, which is kept between frames, so the damage only has to cover where its answer
/// *changed*.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reads {
    /// Its own isolated target, populated by this frame alone.
    OwnTarget,
    /// The composite beneath it, which outlives the frame.
    WhatIsBeneath,
}

/// What one composite writes, and what it reads to do it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReadExtent {
    /// The pixels the composite writes.
    pub bounds: Rect<DevicePx, Device>,
    /// The pixels it reads, which is [`ReadExtent::bounds`] inflated by the filter chain's reach.
    pub source: Rect<DevicePx, Device>,
    /// Where those pixels come from.
    pub reads: Reads,
}

impl ReadExtent {
    /// Whether the composite reads exactly what it writes, which is true of nearly every one.
    ///
    /// A per-pixel filter, a blend mode and plain opacity all read only the pixel being written, and
    /// expanding damage for them would cost a great deal and buy nothing: inside a damage rectangle
    /// that pixel is being redrawn from the clear anyway.
    pub fn is_degenerate(&self) -> bool {
        self.source == self.bounds
    }
}

/// What `frag` reads, or nothing when it reads only what it writes.
///
/// It is a pure function of the fragment's geometry and its box's computed filter chain, which is
/// what lets the expansion evaluate it before a single primitive has been emitted.
///
/// `scale` is how many device pixels one CSS pixel is, and it has to be the frame's own: a filter's
/// reach is a length, so a chain converted at one scale and a fragment's ink measured at another
/// disagree about the same blur by exactly that factor — and the disagreement is in the direction
/// that leaves a region read but never repainted.
///
/// The extent is deliberately conservative in one respect: a filtered fragment's ink already carries
/// the chain's reach, and the source is computed by inflating that ink again. So the region is never
/// too small, which is the only direction that is a bug — too small leaves a faded edge or a
/// smearing panel, and too large costs pixels.
pub fn read_extent_of(store: &LayoutStore, frag: FragKey, scale: f32) -> Option<ReadExtent> {
    let fragment = store.fragment(frag)?;
    let node = store.get(fragment.box_)?;
    let declared = store.custom_properties(fragment.box_, &node.style);
    let own = filter::own_with(&node.style, declared, scale);
    // A fragment carrying both is read as the stricter of the two: the content filter's own target
    // still has to be painted whole, and there is nothing to be gained by treating half a chain
    // one way and half the other.
    let reads = if own.is_empty() {
        Reads::WhatIsBeneath
    } else {
        Reads::OwnTarget
    };
    let mut chain = own;
    chain.extend(filter::backdrop_with(&node.style, declared, scale));
    if chain.is_empty() {
        return None;
    }
    // A group writes everything below it; a backdrop writes its own box. The union of the two is
    // what a fragment carrying both writes, and it is what the subtree ink already is.
    let bounds = fragment.subtree_ink;
    let source = read_extent(bounds, &chain);
    let extent = ReadExtent {
        bounds,
        source,
        reads,
    };
    (!extent.is_degenerate()).then_some(extent)
}

/// The rectangle a fragment has to be tested against before it is skipped.
///
/// It is the fragment's ink unioned with whatever it reads, so a composite that samples outside what
/// it writes is never culled out from under its own expansion. Once the expansion has run this is
/// belt and braces — the damage already contains the source, which contains the bounds, which
/// contains the ink — and it is kept because it calls the same function the expansion did.
pub fn cull_rect(store: &LayoutStore, fragment: &Fragment, scale: f32) -> Rect<DevicePx, Device> {
    match read_extent_of(store, fragment.key, scale) {
        Some(extent) => fragment.ink.union(extent.source),
        None => fragment.ink,
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};

    use super::{ReadExtent, Reads};

    /// A rectangle at the origin.
    fn rect(width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    #[test]
    fn an_extent_that_reads_what_it_writes_is_degenerate() {
        let bounds = rect(10.0, 10.0);
        assert!(
            ReadExtent {
                bounds,
                source: bounds,
                reads: Reads::OwnTarget,
            }
            .is_degenerate()
        );
        assert!(
            !ReadExtent {
                bounds,
                source: rect(20.0, 20.0),
                reads: Reads::OwnTarget,
            }
            .is_degenerate()
        );
    }
}
