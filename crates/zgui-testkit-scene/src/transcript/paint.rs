//! Paint references, resolved through the scene's table rather than printed as indices.
//!
//! An index is not a regression artifact: `fill=#7` says nothing about what a reviewer is looking
//! at, and two frames whose paints interned in a different order would diff on every primitive
//! while drawing identically. So a reference is rendered as the paint it resolves to.

use zgui_color::{Color, ColorSpace, GradientStop, HueInterpolation};
use zgui_scene::{GradientKind, Paint, PaintKind, PaintRef, PaintTable, TextPaint, TextPaintTable};
use zgui_scene::{PaintId, PaintSlot};

use crate::text::number::{float, list, rect};
use crate::transcript::tile;

/// One colour, in its own space.
///
/// The space travels with the components because they are meaningless without it: `oklch(0.7, 0.1,
/// 320)` and `srgb(0.7, 0.1, 320)` are not the same colour and would otherwise print alike.
pub fn color(color: Color) -> String {
    let components = color.components();
    format!(
        "{}({}, {}, {}, {})",
        space(color.space()),
        float(components[0]),
        float(components[1]),
        float(components[2]),
        float(color.alpha())
    )
}

/// Premultiplied, gamma-encoded sRGB as an instance carries it.
pub fn premultiplied(components: [f32; 4]) -> String {
    format!("premul_srgb{}", list(&components))
}

/// A colour space's name.
pub fn space(space: ColorSpace) -> &'static str {
    match space {
        ColorSpace::Srgb => "srgb",
        ColorSpace::SrgbLinear => "srgb_linear",
        ColorSpace::Hsl => "hsl",
        ColorSpace::Hwb => "hwb",
        ColorSpace::Lab => "lab",
        ColorSpace::Lch => "lch",
        ColorSpace::Oklab => "oklab",
        ColorSpace::Oklch => "oklch",
        ColorSpace::DisplayP3 => "display_p3",
        ColorSpace::A98Rgb => "a98_rgb",
        ColorSpace::ProPhotoRgb => "prophoto_rgb",
        ColorSpace::Rec2020 => "rec2020",
        ColorSpace::XyzD50 => "xyz_d50",
        ColorSpace::XyzD65 => "xyz_d65",
    }
}

/// How a polar space walks the hue circle.
pub fn hue(hue: HueInterpolation) -> &'static str {
    match hue {
        HueInterpolation::Shorter => "shorter",
        HueInterpolation::Longer => "longer",
        HueInterpolation::Increasing => "increasing",
        HueInterpolation::Decreasing => "decreasing",
    }
}

/// One gradient stop.
pub fn stop(stop: GradientStop) -> String {
    format!("{}@{}", color(stop.color), float(stop.offset))
}

/// A paint reference, resolved through `table`.
///
/// A reference whose family disagrees with the entry it points at is rendered as the disagreement
/// rather than as either half, because that is a real defect — a shader told "solid" about a
/// gradient reads the wrong storage — and a transcript that quietly printed the entry would hide it.
pub fn reference(table: &PaintTable, reference: PaintRef) -> String {
    let Some(id) = reference.id() else {
        return "none".to_owned();
    };
    let declared = kind(reference.kind);
    if table.get(id).is_none() {
        return format!("{declared}#{} <missing>", id.index());
    }
    // The family the *entry* implies, read off the table rather than re-derived: the entry's own
    // discriminant is numbered differently from the one an instance carries, and comparing the two
    // numbers directly would report every solid fill as a disagreement.
    let actual = kind(table.reference(id).kind);
    if declared != actual {
        return format!(
            "<mismatched> declared={declared} entry={}",
            paint(table, id).unwrap_or_default()
        );
    }
    paint(table, id).unwrap_or_default()
}

/// The entry `id` resolves to.
pub fn paint(table: &PaintTable, id: PaintId) -> Option<String> {
    Some(match table.get(id)? {
        Paint::Solid(value) => format!("solid {}", color(*value)),
        Paint::Gradient {
            kind,
            stops,
            space: interpolation,
            hue: circle,
            repeating,
        } => {
            let rendered: Vec<String> = stops.iter().copied().map(stop).collect();
            format!(
                "{} in {} {} stops=[{}]{}",
                gradient(kind),
                space(*interpolation),
                hue(*circle),
                rendered.join(", "),
                if *repeating { " repeating" } else { "" }
            )
        }
        Paint::Image {
            tile: atlas,
            destination,
            transform,
            repeating,
        } => format!(
            "image {} dest={} transform=#{}{}",
            tile::of(*atlas),
            rect([
                destination.origin.x.0,
                destination.origin.y.0,
                destination.size.width.0,
                destination.size.height.0,
            ]),
            transform.index(),
            if *repeating { " repeating" } else { "" }
        ),
    })
}

/// A gradient's shape.
pub fn gradient(kind: &GradientKind) -> String {
    match kind {
        GradientKind::Linear { start, end } => format!(
            "linear from=({}, {}) to=({}, {})",
            float(start.x.0),
            float(start.y.0),
            float(end.x.0),
            float(end.y.0)
        ),
        GradientKind::Radial {
            center,
            radius_x,
            radius_y,
        } => format!(
            "radial at=({}, {}) radii=({}, {})",
            float(center.x.0),
            float(center.y.0),
            float(*radius_x),
            float(*radius_y)
        ),
        GradientKind::Conic { center, from_angle } => format!(
            "conic at=({}, {}) from={}rad",
            float(center.x.0),
            float(center.y.0),
            float(*from_angle)
        ),
    }
}

/// A text brush slot, resolved through `table`.
pub fn slot(table: &TextPaintTable, slot: PaintSlot) -> String {
    match table.get(slot) {
        Some(TextPaint { color: components }) => {
            format!("slot#{} {}", slot.index(), premultiplied(*components))
        }
        None => format!("slot#{} <missing>", slot.index()),
    }
}

/// A paint family's name, from the discriminant an instance or an entry carries.
fn kind(tag: u32) -> &'static str {
    match tag {
        tag if tag == PaintKind::None as u32 => "none",
        tag if tag == PaintKind::Solid as u32 => "solid",
        tag if tag == PaintKind::Gradient as u32 => "gradient",
        tag if tag == PaintKind::Image as u32 => "image",
        _ => "<unknown>",
    }
}

#[cfg(test)]
mod tests {
    use zgui_color::Color;
    use zgui_scene::{PaintId, PaintKind, PaintRef, PaintTable};

    use super::{color, reference};

    #[test]
    fn a_colours_space_travels_with_its_components() {
        let plum = Color::new(zgui_color::ColorSpace::Oklch, [0.7, 0.1, 320.0], 1.0);
        assert_eq!(color(plum), "oklch(0.7, 0.1, 320, 1)");
    }

    #[test]
    fn a_reference_renders_the_paint_and_not_the_index() {
        let mut table = PaintTable::new();
        let id = table.solid(Color::srgb(1.0, 0.0, 0.0, 1.0));
        assert_eq!(
            reference(&table, PaintRef::solid(id)),
            "solid srgb(1, 0, 0, 1)"
        );
        assert_eq!(reference(&table, PaintRef::NONE), "none");
    }

    #[test]
    fn a_family_that_disagrees_with_its_entry_is_rendered_as_the_disagreement() {
        // The defect this exists to make visible: the family travels in the instance and the entry
        // lives in the table, so the two can be written out of step, and a shader believes the
        // instance. A transcript that printed the entry would show nothing wrong.
        let mut table = PaintTable::new();
        let id = table.solid(Color::BLACK);
        let lying = PaintRef::new(PaintKind::Gradient, id);
        assert!(reference(&table, lying).starts_with("<mismatched>"));
    }

    #[test]
    fn an_index_resolving_to_nothing_says_so() {
        let table = PaintTable::new();
        assert_eq!(
            reference(&table, PaintRef::solid(PaintId(7))),
            "solid#7 <missing>"
        );
    }
}
