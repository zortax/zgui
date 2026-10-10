//! The rectangle primitives: quads, shaded quads, shadows and decoration lines, and the marks
//! that draw recognised shapes.

use zgui_scene::{ClipId, Decoration, MarkFlags, MarkItem, Quad, Scene, ShadedQuad, Shadow};

use crate::text::number::{all_zero, float, list, rect};
use crate::transcript::paint;
use crate::transcript::primitive::{style, suffix};

/// A rounded, bordered rectangle.
pub fn quad(scene: &Scene, quad: &Quad) -> String {
    let mut line = format!(
        "quad order={} bounds={} fill={}",
        quad.order,
        rect(quad.bounds),
        paint::reference(&scene.paints, quad.fill)
    );
    if !all_zero(&quad.border) {
        line.push_str(&format!(
            " border={} stroke={} style={}",
            list(&quad.border),
            paint::reference(&scene.paints, quad.stroke),
            style::border(quad.style)
        ));
    }
    if !all_zero(&quad.radii) {
        line.push_str(&format!(" radii={}", list(&quad.radii)));
    }
    // Printed only when it is not zero, because a quad drawn where its paints were resolved has
    // nothing to say here and every line that named an origin of zero would say the same nothing.
    if !all_zero(&quad.paint_origin) {
        line.push_str(&format!(" paint_origin={}", list(&quad.paint_origin)));
    }
    line.push_str(&suffix(
        scene,
        quad.clip_id(),
        scene.spatial.at(quad.transform),
    ));
    line
}

/// A rectangle an application's own shader draws.
pub fn shaded(scene: &Scene, shaded: &ShadedQuad) -> String {
    let mut line = format!(
        "shaded order={} bounds={} shader={} params={}",
        shaded.order,
        rect(shaded.bounds),
        shaded.shader,
        shaded.params
    );
    if !all_zero(&shaded.border) {
        line.push_str(&format!(
            " border={} stroke={}",
            list(&shaded.border),
            paint::reference(&scene.paints, shaded.stroke)
        ));
    }
    if shaded.fill != zgui_scene::PaintRef::NONE {
        line.push_str(&format!(
            " fill={}",
            paint::reference(&scene.paints, shaded.fill)
        ));
    }
    if !all_zero(&shaded.radii) {
        line.push_str(&format!(" radii={}", list(&shaded.radii)));
    }
    if !all_zero(&shaded.paint_origin) {
        line.push_str(&format!(" paint_origin={}", list(&shaded.paint_origin)));
    }
    if shaded.opacity != 1.0 {
        line.push_str(&format!(" opacity={}", float(shaded.opacity)));
    }
    line.push_str(&suffix(
        scene,
        shaded.clip_id(),
        scene.spatial.at(shaded.transform),
    ));
    line
}

/// A box shadow, drop or inset.
pub fn shadow(scene: &Scene, shadow: &Shadow) -> String {
    let mut line = format!(
        "shadow order={} bounds={} blur={} color={} element={}",
        shadow.order,
        rect(shadow.bounds),
        float(shadow.blur),
        paint::premultiplied(shadow.color),
        rect(shadow.element_bounds)
    );
    if !all_zero(&shadow.radii) {
        line.push_str(&format!(" radii={}", list(&shadow.radii)));
    }
    if !all_zero(&shadow.element_radii) {
        line.push_str(&format!(" element_radii={}", list(&shadow.element_radii)));
    }
    if shadow.inset != 0 {
        // A drop shadow's shape is its bounds less the blur's reach. An inset shadow's hole is
        // independent of its bounds, so it is printed.
        line.push_str(&format!(" inset hole={}", rect(shadow.shape_bounds)));
    }
    line.push_str(&suffix(
        scene,
        ClipId(shadow.clip),
        scene.spatial.at(shadow.transform),
    ));
    line
}

/// A text decoration line.
pub fn decoration(scene: &Scene, decoration: &Decoration) -> String {
    let mut line = format!(
        "decoration order={} bounds={} color={} thickness={} style={}",
        decoration.order,
        rect(decoration.bounds),
        paint::premultiplied(decoration.color),
        float(decoration.thickness),
        style::decoration(decoration.style)
    );
    line.push_str(&suffix(
        scene,
        ClipId(decoration.clip),
        scene.spatial.at(decoration.transform),
    ));
    line
}

/// Recognised shapes over a shared payload: the counts and the flags, never the payload.
pub fn marks(scene: &Scene, mark: &MarkItem) -> String {
    let cap = |shift: u32| match (mark.flags >> shift) & 3 {
        MarkFlags::BUTT => "butt",
        MarkFlags::SQUARE => "square",
        MarkFlags::ROUND => "round",
        _ => "<unknown>",
    };
    let mut line = format!(
        "marks order={} bounds={} paint={} discs={} boxes={} vertices={}",
        mark.order,
        rect(mark.bounds),
        paint::reference(&scene.paints, mark.paint),
        mark.discs,
        mark.boxes,
        mark.vertices
    );
    if mark.glyphs > 0 {
        line.push_str(&format!(" glyphs={} tiles={}", mark.glyphs, mark.tiles));
    }
    if mark.is_union() {
        line.push_str(" union");
    }
    if mark.flags & MarkFlags::SCREEN != 0 {
        line.push_str(" screen");
    }
    if mark.flags & MarkFlags::SQUARE_DISCS != 0 {
        line.push_str(" square_discs");
    }
    if mark.axes != [1.0, 0.0, 0.0, 1.0] {
        line.push_str(&format!(" axes={}", list(&mark.axes)));
    }
    if mark.vertices > 0 {
        line.push_str(&format!(
            " half_width={} caps={}/{}",
            float(mark.half_width),
            cap(MarkFlags::START_CAP_SHIFT),
            cap(MarkFlags::END_CAP_SHIFT)
        ));
    }
    if !all_zero(&mark.origin) {
        line.push_str(&format!(" origin={}", list(&mark.origin)));
    }
    if !all_zero(&mark.paint_origin) {
        line.push_str(&format!(" paint_origin={}", list(&mark.paint_origin)));
    }
    line.push_str(&suffix(
        scene,
        mark.clip_id(),
        scene.spatial.at(mark.transform),
    ));
    line
}
