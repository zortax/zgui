//! Where one run's glyphs land on the surface.
//!
//! # The one number every glyph is placed from
//!
//! A glyph's horizontal position is fractional and arrives in two parts — the line box's own left
//! edge, which layout decided, and the glyph's advance along it, which shaping decided. Neither is
//! whole, and only their sum is where the glyph goes. That sum is split once, into the pixel the
//! tile is drawn at and the phase it is rasterised for, and both the cache key and the rectangle are
//! read off that single split. Splitting the two parts separately — taking the phase from the
//! advance and the pixel from the sum — is what puts a letter nearly a whole pixel from where the
//! shaper asked for it, crowding one neighbour and opening a gap to the other.

use zgui_geom::{Device, DevicePx, Point};
use zgui_text::{GlyphRaster, PenPosition, ShapedRun};

use crate::content::glyphs::cache::Rasterising;
use crate::emit::text::PlacedGlyph;

/// Places one run's glyphs, rasterising and uploading whatever is not cached yet.
///
/// `origin` is the line box's top-left corner in absolute device pixels; the run's own positions
/// are relative to it. Glyphs that could not be rasterised or could not be given a tile are left
/// out of the result rather than reported, so a caller draws what there is.
pub(crate) fn place(
    into: &mut Rasterising<'_>,
    raster: &dyn GlyphRaster,
    run: &ShapedRun<'_>,
    style: zgui_text::RasterStyle,
    origin: Point<DevicePx, Device>,
    out: &mut Vec<PlacedGlyph>,
) {
    for glyph in run.glyphs {
        crate::content::probe::placed();
        let position = PenPosition::of(origin.x.0 + glyph.x);
        let key = run.key_at(glyph.glyph, position.offset(), style);
        let Some(rasterised) = into.tile_for(raster, &key) else {
            continue;
        };
        out.push(PlacedGlyph {
            resource: rasterised.tile.into(),
            bounds: position.bounds(origin.y.0 + glyph.y, rasterised.placement, rasterised.size),
        });
    }
}

#[cfg(test)]
mod tests;
