//! Positioned glyphs from the fixed face, and the rasteriser that fills them in.
//!
//! This is the half of the shaping contract painting reads, done the same way the measuring half
//! is: one glyph per character, every one of them a filled rectangle of the cluster's advance and
//! the face's ascent. Nothing about it is typography. What it buys is that a *painting* test can
//! state where a glyph must land — the third character of the second line is at eight times two
//! plus the line's own left edge — and be wrong only if the pipeline is.

use std::sync::Arc;

use zgui_geom::{DevicePx, Point, Size};
use zgui_text::kurbo::{BezPath, Rect as KurboRect, Shape};
use zgui_text::metrics::fixed::ratio;
use zgui_text::{
    FaceId, GlyphFormat, GlyphImage, GlyphKey, GlyphOutline, GlyphRaster, OutlineKey, ShapedGlyph,
    ShapedParagraph, ShapedRun,
};

use crate::shaper::cluster::MonoLayout;

/// The one face this shaper draws from.
///
/// A single handle, because there is a single face: a test that needs fallback, or two faces at
/// once, needs a real font engine.
pub const FACE: FaceId = FaceId(0);

/// The glyph index one character is drawn as.
///
/// The scalar value itself, truncated, so that a test can name the glyph it expects without
/// consulting a font table — and so that two different characters are never one glyph.
pub fn glyph_id(character: char) -> u16 {
    (character as u32 & 0xffff) as u16
}

/// Visits the runs of one line, positioned against the line box's top-left corner.
///
/// A run ends where the brush changes, which is the only thing that varies across a line here: the
/// fixed face has one size and one shape, so two clusters differ in nothing else. Atomic inline
/// boxes are not interleaved with the glyphs, because this shaper's own line geometry does not
/// interleave them either.
pub(crate) fn visit_line(
    shaped: &ShapedParagraph<MonoLayout>,
    line: u16,
    visit: &mut dyn FnMut(ShapedRun<'_>),
) {
    let engine = &shaped.engine;
    let index = line as usize;
    let (Some((start, end)), Some(geometry)) =
        (engine.lines.get(index).copied(), engine.geometry.get(index))
    else {
        return;
    };
    let baseline = geometry.baseline.0 - geometry.top.0;
    // Device pixels, which is what a run's size is defined to be and what the glyph key built
    // from it is looked up under. The strut is in CSS pixels, so the ratio the paragraph was
    // shaped at is what converts it — and leaving it out asks the rasteriser for a one-times
    // glyph to be drawn into a box the layout sized at the real ratio.
    let size = shaped.strut().font_size.0 * shaped.engine.scale;
    let mut pen = 0.0;
    let mut glyphs: Vec<ShapedGlyph> = Vec::new();
    let mut brush = None;
    for cluster in &engine.clusters[start.min(end)..end] {
        if brush.is_some_and(|held| held != cluster.brush) {
            emit(visit, size, brush.take().expect("just checked"), &glyphs);
            glyphs.clear();
        }
        brush = Some(cluster.brush);
        let character = shaped.text()[cluster.offset..]
            .chars()
            .next()
            .unwrap_or(' ');
        glyphs.push(ShapedGlyph {
            glyph: glyph_id(character),
            x: pen,
            y: baseline,
        });
        pen += cluster.advance.0;
    }
    if let Some(brush) = brush {
        emit(visit, size, brush, &glyphs);
    }
}

/// Hands one style-uniform stretch of a line to the visitor.
fn emit(
    visit: &mut dyn FnMut(ShapedRun<'_>),
    size: f32,
    brush: zgui_text::Brush,
    glyphs: &[ShapedGlyph],
) {
    if glyphs.is_empty() {
        return;
    }
    visit(ShapedRun {
        face: FACE,
        size,
        synthetic_bold: 0.0,
        synthetic_slant: 0.0,
        has_color: false,
        brush,
        glyphs,
    });
}

/// A rasteriser with no font files, matching [`MonoShaper`](crate::MonoShaper)'s fixed face.
///
/// Every glyph is a solid rectangle: as wide as the cluster's advance, as tall as the face's
/// ascent, sitting on the baseline with no bearing. Space is the one exception and it rasterises
/// to nothing, which is what a space does in a real face and what keeps a run of prose from
/// drawing a solid bar.
///
/// ```
/// use zgui_testkit_scene::{MonoRaster, glyph_id};
/// use zgui_text::{FaceId, GlyphKey, GlyphRaster, RasterStyle, SubpixelOffset};
///
/// let raster = MonoRaster::new();
/// let key = GlyphKey::new(FaceId(0), glyph_id('a'), 16.0, SubpixelOffset(0), RasterStyle::Grayscale);
/// let image = raster.raster(&key).expect("the fixed face draws every glyph");
/// assert_eq!(image.size.width, 8, "half the size, as the fixed face's advance is");
/// assert!(image.is_well_formed());
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct MonoRaster;

impl MonoRaster {
    /// A rasteriser over the fixed face.
    pub fn new() -> Self {
        Self
    }
}

impl GlyphRaster for MonoRaster {
    fn raster(&self, key: &GlyphKey) -> Option<GlyphImage> {
        if key.face != FACE {
            return None;
        }
        let size = key.size();
        let width = (size * ratio::ZERO_ADVANCE).round().max(0.0) as u32;
        let height = (size * ratio::ASCENT).round().max(0.0) as u32;
        let (width, height) = if key.glyph == glyph_id(' ') {
            (0, 0)
        } else {
            (width, height)
        };
        let format = match key.style {
            zgui_text::RasterStyle::Grayscale => GlyphFormat::Mono,
            zgui_text::RasterStyle::Subpixel => GlyphFormat::Subpixel,
            zgui_text::RasterStyle::Color => GlyphFormat::Color,
        };
        let texels = (width * height) as usize * format.bytes_per_pixel();
        Some(GlyphImage {
            size: Size::new(width, height),
            // The rectangle sits directly on the baseline and starts at the pen, so its top edge
            // is its whole height above the origin and its left edge is at zero.
            placement: Point::new(DevicePx(0.0), DevicePx(height as f32)),
            format,
            bytes: vec![u8::MAX; texels],
        })
    }

    /// The same rectangle the tile is, as a curve.
    ///
    /// The two paths have to agree about what a glyph *is*, or a test that promotes a run to
    /// outlines would be comparing two different faces and could not tell a promotion from a
    /// regression. So this is the tile's rectangle exactly: from the pen to the advance, from the
    /// baseline up to the ascent, in the space outlines are returned in — y upward from the
    /// baseline is y negative here.
    fn outline(&self, key: &OutlineKey) -> Option<GlyphOutline> {
        if key.face != FACE {
            return None;
        }
        let size = f64::from(key.size());
        let width = (size * f64::from(ratio::ZERO_ADVANCE)).round().max(0.0);
        let height = (size * f64::from(ratio::ASCENT)).round().max(0.0);
        if key.glyph == glyph_id(' ') {
            return Some(Arc::new(BezPath::new()));
        }
        let lean = f64::from(key.synthetic_slant().to_radians().tan());
        let mut path = KurboRect::new(0.0, -height, width, 0.0).to_path(0.1);
        if lean != 0.0 {
            // The same shear a real face's curves are given: rightwards in proportion to how far
            // above the baseline a point is.
            path.apply_affine(zgui_text::kurbo::Affine::new([
                1.0, 0.0, -lean, 1.0, 0.0, 0.0,
            ]));
        }
        Some(Arc::new(path))
    }
}

#[cfg(test)]
mod tests {
    use zgui_text::{FaceId, GlyphKey, GlyphRaster, RasterStyle, SubpixelOffset};

    use super::{MonoRaster, glyph_id};

    #[test]
    fn a_space_rasterises_to_nothing_and_a_letter_does_not() {
        let raster = MonoRaster::new();
        let at = |character: char| {
            raster
                .raster(&GlyphKey::new(
                    FaceId(0),
                    glyph_id(character),
                    16.0,
                    SubpixelOffset(0),
                    RasterStyle::Grayscale,
                ))
                .expect("the fixed face has every glyph")
        };
        assert!(at(' ').is_empty());
        assert!(!at('a').is_empty());
    }

    #[test]
    fn a_handle_no_fixed_face_issued_has_no_glyphs() {
        assert!(
            MonoRaster::new()
                .raster(&GlyphKey::new(
                    FaceId(9),
                    1,
                    16.0,
                    SubpixelOffset(0),
                    RasterStyle::Grayscale
                ))
                .is_none()
        );
    }
}
