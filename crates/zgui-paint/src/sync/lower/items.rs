//! zgui primitives as compositor items.

use compositor_types::{
    BorderStyle, ChunkRecorder, Color, FontId, Glyph, GlyphFlags, GlyphRun, Line, LineStyle, Quad,
    Shadow,
};
use zgui_scene::CapturedRun;

use crate::sync::lower::refs::{Recorded, Refs};

/// Appends a quad.
pub(crate) fn quad(
    chunk: &mut ChunkRecorder<'_>,
    refs: &Refs,
    recorded: &mut Recorded,
    quad: &zgui_scene::Quad,
) {
    let item = Quad {
        bounds: quad.bounds,
        radii: quad.radii,
        shapes: [quad.shape; 4],
        border: quad.border,
        fill: recorded.paint(refs, chunk, quad.fill),
        stroke: recorded.paint(refs, chunk, quad.stroke),
        paint_origin: quad.paint_origin,
        clip: recorded.clip(refs, chunk, quad.clip),
        space: recorded.space(refs, chunk, quad.transform),
        // The low byte holds the border style, and both vocabularies number it alike.
        style: BorderStyle((quad.style & 0xff) as u8),
        _pad: [0; 3],
    };
    chunk.quad(item);
}

/// Appends a box shadow.
pub(crate) fn shadow(
    chunk: &mut ChunkRecorder<'_>,
    refs: &Refs,
    recorded: &mut Recorded,
    shadow: &zgui_scene::Shadow,
) {
    let item = Shadow {
        shape_bounds: shadow.shape_bounds,
        radii: shadow.radii,
        caster_bounds: shadow.element_bounds,
        caster_radii: shadow.element_radii,
        shapes: [shadow.shape; 4],
        color: Color(shadow.color),
        blur: shadow.blur,
        clip: recorded.clip(refs, chunk, shadow.clip),
        space: recorded.space(refs, chunk, shadow.transform),
        inset: shadow.inset,
    };
    chunk.shadow(item);
}

/// Appends a decoration line.
pub(crate) fn line(
    chunk: &mut ChunkRecorder<'_>,
    refs: &Refs,
    recorded: &mut Recorded,
    line: &zgui_scene::Decoration,
) {
    let item = Line {
        bounds: line.bounds,
        color: Color(line.color),
        thickness: line.thickness,
        clip: recorded.clip(refs, chunk, line.clip),
        space: recorded.space(refs, chunk, line.transform),
        // The low byte holds the line style, and both vocabularies number it alike.
        style: LineStyle((line.style & 0xff) as u8),
        _pad: [0; 3],
    };
    chunk.line(item);
}

/// Appends a glyph run, and returns whether it drew a variation instance the compositor was not
/// given the coordinates of.
pub(crate) fn glyph_run(
    chunk: &mut ChunkRecorder<'_>,
    refs: &Refs,
    recorded: &mut Recorded,
    run: &CapturedRun,
    glyphs: &mut Vec<Glyph>,
) -> bool {
    glyphs.clear();
    glyphs.extend(run.glyphs.iter().map(|&(id, x, y)| Glyph {
        id: u32::from(id),
        x,
        y,
    }));
    let mut flags = GlyphFlags::HINT;
    if run.subpixel {
        flags |= GlyphFlags::SUBPIXEL;
    }
    if run.silhouette {
        flags |= GlyphFlags::SILHOUETTE;
    }
    let item = GlyphRun {
        // A face id is the font id, because the text engine's face table is the font registry.
        font: FontId(run.face),
        size: run.size,
        glyphs: chunk.glyphs(glyphs),
        coords: chunk.coords(&[]),
        paint: recorded.solid(refs, chunk, run.color),
        embolden: run.embolden,
        skew: run.skew,
        clip: recorded.clip(refs, chunk, run.clip),
        space: recorded.space(refs, chunk, run.transform),
        flags,
        _pad: [0; 1],
        palette: 0,
    };
    chunk.glyph_run(item);
    run.instance != 0
}
