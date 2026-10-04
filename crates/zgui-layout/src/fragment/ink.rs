//! How far outside its border box a fragment paints.
//!
//! Damage is computed from this rectangle, and under-reporting it is the single most common source
//! of stale pixels: a shadow, an outline or a blur that reaches further than the rectangle says
//! leaves a trail behind whenever the fragment moves. So every decoration that reaches outside the
//! border box is accounted for here, and the rectangle only ever grows.

use zgui_css::ComputedStyle;
use zgui_css::values::border::OutlineStyleValue;
use zgui_css::values::color::{current, to_color};
use zgui_css::values::image::ImageValue;
use zgui_geom::{Device, DevicePx, Edges, Rect};
use zgui_scene::{Filter, read_extent};

use crate::fragment::filter;

/// Everything a box paints, starting from its border box.
///
/// The three contributions are independent and each is a union rather than a maximum: an outline
/// can reach further than a shadow on one side and less on another.
pub fn of(
    style: &ComputedStyle,
    border_box: Rect<DevicePx, Device>,
    scale: f32,
) -> Rect<DevicePx, Device> {
    let mut ink = border_box;
    if let Some(shadows) = shadow_extent(style, border_box, scale) {
        ink = ink.union(shadows);
    }
    if let Some(outline) = outline_extent(style, border_box, scale) {
        ink = ink.union(outline);
    }
    // A filter is applied to everything the box already painted, so it spreads what has been
    // accumulated so far rather than the border box alone.
    let filters = filter::own(style, scale);
    if !filters.is_empty() {
        ink = bleed(ink, &filters);
    }
    ink
}

/// How far a glyph can lean past its advance, as a part of its content area's height.
///
/// The tangent of the fourteen degrees a synthesized italic leans at. A designed italic leans less.
const LEAN: f32 = 0.25;

/// Everything one line of text paints, in the space its box is laid out in.
///
/// `line` is the line box, `extents` is how far it reaches either side of its baseline, and `ink`
/// is how far its glyphs reach. A glyph leans past its advance, and painting rounds its baseline
/// and its pen to whole pixels, so the rectangle grows by the lean and by one device pixel on every
/// side. Text shadows add their own offset copies. Every side grows by a whole number of pixels, so
/// the rectangle moves by exactly what the line box moves by.
pub fn of_line(
    style: &ComputedStyle,
    line: Rect<DevicePx, Device>,
    extents: crate::inline::strut::Extents,
    ink: crate::inline::strut::Extents,
    scale: f32,
) -> Rect<DevicePx, Device> {
    let lean = LEAN * ink.height() + 1.0;
    let glyphs = [
        ink.above - extents.above + 1.0,
        lean,
        ink.below - extents.below + 1.0,
        lean,
    ];
    let mut reach = glyphs;
    for shadow in &*style.get_inherited_text().text_shadow.0 {
        let blur = Filter::BLUR_EXTENT * (shadow.blur.0.px() * scale).max(0.0) / 2.0;
        let x = shadow.horizontal.px() * scale;
        let y = shadow.vertical.px() * scale;
        let copy = [
            glyphs[0] + blur - y,
            glyphs[1] + blur + x,
            glyphs[2] + blur + y,
            glyphs[3] + blur - x,
        ];
        for (side, copied) in reach.iter_mut().zip(copy) {
            *side = side.max(copied);
        }
    }
    let [top, right, bottom, left] = reach.map(|side| DevicePx(side.max(0.0).ceil()));
    line.outset(Edges {
        top,
        right,
        bottom,
        left,
    })
}

/// How far a filter chain spreads what it is applied to.
///
/// The same reach that decides which pixels a chain *reads* also decides how far the result of
/// applying it is painted, so both call one function.
pub fn bleed(rect: Rect<DevicePx, Device>, filters: &[Filter]) -> Rect<DevicePx, Device> {
    read_extent(rect, filters)
}

/// Whether the box's own painting is nothing at all.
///
/// Everything a plain box can paint is asked in turn: the background colour and its layers, the
/// border (by its snapped widths — a styled border of no width paints no pixel), the outline,
/// and the shadows, inset and outer alike. The colour is resolved so `currentColor` at any
/// visible alpha answers no. What this deliberately does not ask about — transforms, clips,
/// filters, stacking — belongs to the flags a consumer reads beside this.
pub fn paints_nothing(style: &ComputedStyle, border: Edges<DevicePx>) -> bool {
    let background = style.get_background();
    border == Edges::ZERO
        && to_color(
            &background
                .background_color
                .resolve_to_absolute(current(style)),
        )
        .alpha()
            == 0.0
        && background
            .background_image
            .0
            .iter()
            .all(|image| matches!(image, ImageValue::None))
        && style.get_effects().box_shadow.0.is_empty()
        && {
            let outline = style.get_outline();
            outline.outline_style == OutlineStyleValue::none()
                || outline.outline_width.0.to_f32_px() == 0.0
        }
}

/// The union of every outer box shadow, or nothing when there are none.
///
/// Inset shadows are painted inside the padding box and never reach outside it, so they contribute
/// nothing here.
fn shadow_extent(
    style: &ComputedStyle,
    border_box: Rect<DevicePx, Device>,
    scale: f32,
) -> Option<Rect<DevicePx, Device>> {
    let mut extent: Option<Rect<DevicePx, Device>> = None;
    for shadow in &*style.get_effects().box_shadow.0 {
        if shadow.inset {
            continue;
        }
        let blur = Filter::BLUR_EXTENT * (shadow.base.blur.0.px() * scale).max(0.0) / 2.0;
        let spread = shadow.spread.px() * scale;
        let reach = blur + spread;
        let rect = border_box
            .translate(zgui_geom::Size::new(
                DevicePx(shadow.base.horizontal.px() * scale),
                DevicePx(shadow.base.vertical.px() * scale),
            ))
            .outset(Edges {
                top: DevicePx(reach),
                right: DevicePx(reach),
                bottom: DevicePx(reach),
                left: DevicePx(reach),
            });
        extent = Some(match extent {
            Some(held) => held.union(rect),
            None => rect,
        });
    }
    extent
}

/// The rectangle an outline covers, or nothing when the box draws none.
///
/// An outline is drawn outside the border box, offset by `outline-offset`, and unlike a border it
/// takes up no space — which is exactly why it has to be in the ink rectangle and cannot be found
/// from the geometry alone.
fn outline_extent(
    style: &ComputedStyle,
    border_box: Rect<DevicePx, Device>,
    scale: f32,
) -> Option<Rect<DevicePx, Device>> {
    let outline = style.get_outline();
    let width = outline.outline_width.0.to_f32_px();
    if outline.outline_style == OutlineStyleValue::none() || width == 0.0 {
        return None;
    }
    let reach = (width + outline.outline_offset.to_f32_px()) * scale;
    if reach <= 0.0 {
        return None;
    }
    Some(border_box.outset(Edges {
        top: DevicePx(reach),
        right: DevicePx(reach),
        bottom: DevicePx(reach),
        left: DevicePx(reach),
    }))
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};
    use zgui_scene::Filter;

    use super::bleed;

    #[test]
    fn a_blur_spreads_ink_three_standard_deviations_on_every_side() {
        let rect: Rect<DevicePx, Device> = Rect::new(
            Point::new(DevicePx(20.0), DevicePx(20.0)),
            Size::new(DevicePx(10.0), DevicePx(10.0)),
        );
        let spread = bleed(rect, &[Filter::Blur(2.0)]);
        assert_eq!(spread.origin, Point::new(DevicePx(14.0), DevicePx(14.0)));
        assert_eq!(spread.size, Size::new(DevicePx(22.0), DevicePx(22.0)));
    }

    #[test]
    fn a_per_pixel_filter_spreads_nothing() {
        let rect: Rect<DevicePx, Device> = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(10.0), DevicePx(10.0)),
        );
        assert_eq!(bleed(rect, &[Filter::Invert(1.0)]), rect);
    }
}
