//! Where one run's outlines land on the surface.
//!
//! # Why there is no phase here
//!
//! A tile is coverage measured on a pixel grid, so a glyph half a pixel to the right is *different
//! pixels* and has to be rasterised again — the whole pen-and-phase apparatus beside this module
//! exists to make that finite. A curve is not measured on anything: it is
//! filled where it is put, by a rasteriser that antialiases analytically. So an outline run is
//! placed at the position the shaper asked for, fractions and all, with nothing rounded and nothing
//! quantised, and the same string drawn a third of a pixel to the right is a third of a pixel to
//! the right.
//!
//! # Why the curve is copied
//!
//! The face's curves are held once per glyph, at the origin, and shared. What a display list needs
//! is a path *where the glyph is*, and the two cannot be the same allocation while one glyph
//! appears twice in a line. So each placed glyph is a translated copy — a few hundred points for a
//! run the atlas already refused to serve, and the alternative is a transform per glyph interned in
//! a table the scene keeps across frames, which grows without bound for text that scrolls.

use std::sync::Arc;

use zgui_geom::{Device, DevicePx, Point};
use zgui_scene::kurbo::{Affine, BezPath};
use zgui_text::{GlyphRaster, ShapedRun};

/// One glyph of a run, as curves already placed on the surface.
#[derive(Clone, Debug)]
pub struct OutlineGlyph {
    /// The curves, in absolute device pixels before the run's own transform.
    ///
    /// Shared because a display list hands the same allocation to a rasteriser, which keeps its
    /// encoding of the geometry under that allocation's identity.
    pub path: Arc<BezPath>,
}

/// Places one run's glyphs as curves, extracting whatever the rasteriser has not extracted yet.
///
/// `origin` is the line box's top-left corner in absolute device pixels; the run's own positions
/// are relative to it. A glyph the face has no curves for, and a glyph whose curves are empty — a
/// space — are left out rather than reported, so a caller draws what there is.
pub(crate) fn place(
    raster: &dyn GlyphRaster,
    run: &ShapedRun<'_>,
    origin: Point<DevicePx, Device>,
    out: &mut Vec<OutlineGlyph>,
) {
    for glyph in run.glyphs {
        crate::content::probe::placed();
        let Some(curves) = raster.outline(&run.outline_key(glyph.glyph)) else {
            continue;
        };
        if curves.is_empty() {
            continue;
        }
        let mut path = BezPath::clone(&curves);
        path.apply_affine(Affine::translate((
            f64::from(origin.x.0 + glyph.x),
            f64::from(origin.y.0 + glyph.y),
        )));
        out.push(OutlineGlyph {
            path: Arc::new(path),
        });
    }
}
