//! Turning one glyph into curves.
//!
//! # Why this is not the bitmap rasteriser with a different output
//!
//! A bitmap is asked for at a phase and is hinted: the outline is shifted by a fraction of a pixel
//! and its stems are pulled onto the pixel grid, and both of those are properties of *where on the
//! screen* the glyph lands. A curve has no pixel grid to be pulled onto and no phase to be shifted
//! by — it is filled wherever it is placed, by a rasteriser that antialiases analytically — so the
//! two requests have different keys and different answers, and hinting one of them would bake a
//! pixel grid into a shape that is about to be rotated.

use std::sync::{Arc, Mutex};

use kurbo::BezPath;
use rustc_hash::FxHashMap;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};
use zgui_text::{GlyphOutline, OutlineKey};

/// How many distinct curves one cache holds before it starts again.
///
/// A run leaves the atlas for a size no cache should hold, so this is bounded by the number of
/// *display-sized* glyphs on the screen at once rather than by the text on the page: a headline, a
/// turned label, a chart's tick marks. Clearing wholesale rather than evicting one entry is what
/// keeps this a few lines instead of a second cache with its own eviction policy — the cost of
/// being wrong is re-extracting a handful of curves in one frame.
const CAPACITY: usize = 512;

/// The curves already extracted, keyed by what decides them.
///
/// The allocation matters as much as the curves: a path rasteriser keeps its encoding of a path
/// under the identity of the `Arc`, so handing back a fresh copy of identical curves every frame
/// would re-encode every glyph of every turned heading, every frame.
#[derive(Debug, Default)]
pub(crate) struct Outlines {
    /// What has been extracted.
    entries: Mutex<FxHashMap<OutlineKey, GlyphOutline>>,
}

impl Outlines {
    /// The curves for `key`, extracting them from `font` if they are not held yet.
    pub(crate) fn get(&self, font: &FontRef<'_>, key: &OutlineKey) -> Option<GlyphOutline> {
        if let Some(held) = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(key)
        {
            return Some(Arc::clone(held));
        }
        let path = Arc::new(extract(font, key)?);
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if entries.len() >= CAPACITY {
            entries.clear();
        }
        entries.insert(*key, Arc::clone(&path));
        Some(path)
    }
}

/// Draws one glyph's curves into a path in device pixels, y downward, origin at the baseline.
fn extract(font: &FontRef<'_>, key: &OutlineKey) -> Option<BezPath> {
    let outlines = font.outline_glyphs();
    let glyph = outlines.get(GlyphId::new(u32::from(key.glyph)))?;
    let mut pen = Pen {
        path: BezPath::new(),
        // A face's outlines grow upward from the baseline and the surface's y grows downward, so
        // the sign of every y is the whole of the conversion. Doing it in the pen rather than by
        // transforming the finished path keeps a glyph's curves in the one space anything outside
        // this crate ever sees them in.
        shear: key.synthetic_slant().to_radians().tan(),
    };
    glyph
        .draw(
            DrawSettings::unhinted(Size::new(key.size()), LocationRef::default()),
            &mut pen,
        )
        .ok()?;
    Some(pen.path)
}

/// Collects a face's outline commands into a path, flipping y and leaning the letter as it goes.
struct Pen {
    /// What is being built.
    path: BezPath,
    /// The tangent of the synthetic slant: how far a point is moved right per pixel above the
    /// baseline. Zero for a face that covers the requested style.
    shear: f32,
}

impl Pen {
    /// One of the face's points, in the surface's space.
    ///
    /// The shear is applied here rather than to the finished path because it is about *this
    /// glyph's* origin: applied to a run it would lean the line of text rather than the letters
    /// on it.
    fn point(&self, x: f32, y: f32) -> kurbo::Point {
        kurbo::Point::new(f64::from(x + self.shear * y), f64::from(-y))
    }
}

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(self.point(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(self.point(x, y));
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.path.quad_to(self.point(cx, cy), self.point(x, y));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.path
            .curve_to(self.point(cx0, cy0), self.point(cx1, cy1), self.point(x, y));
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}
