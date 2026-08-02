//! A box's own decorations: its shadows, its background, its border and its outline.
//!
//! The order here is CSS 2.1 Appendix E's, and it is the whole reason these four live in one
//! function: shadows behind the background, the background behind the border, and the outline
//! last — after the box's descendants, which is why it is emitted separately rather than in the
//! run below.

use smallvec::SmallVec;
use zgui_color::Color;
use zgui_geom::{Corners, Device, DevicePx, Edges, Point, Rect, Size, Vec2};
use zgui_scene::{ClipId, PaintRef, Quad, Scene, Shadow, SpatialId};

use crate::emit::paint::gradient_paint;
use crate::lower::PaintStyle;
use crate::lower::border::inner_radii;

/// Where a box's decorations are drawn.
#[derive(Clone, Copy, Debug)]
pub struct BoxPlacement {
    /// The border box, in absolute device pixels.
    pub border_box: Rect<DevicePx, Device>,
    /// The border widths, already resolved and snapped.
    pub border: Edges<DevicePx>,
    /// The border box's elliptical corner radii.
    pub radii: Corners<Vec2<DevicePx>>,
    /// The chain the box is drawn through.
    pub clip: ClipId,
    /// The transform it is drawn under.
    pub transform: SpatialId,
    /// How many device pixels one CSS pixel is, for resolving a gradient's stop positions.
    pub scale: f32,
}

/// Emits the shadows a box casts outwards, behind everything else it draws.
pub fn outer_shadows(scene: &mut Scene, style: &PaintStyle, placement: BoxPlacement) -> usize {
    shadows(scene, style, placement, false)
}

/// Emits the shadows a box casts inwards, over its background and under its border.
pub fn inset_shadows(scene: &mut Scene, style: &PaintStyle, placement: BoxPlacement) -> usize {
    shadows(scene, style, placement, true)
}

/// Emits one direction's shadows, last written first so the first written ends up on top.
fn shadows(scene: &mut Scene, style: &PaintStyle, placement: BoxPlacement, inset: bool) -> usize {
    let mut pushed = 0;
    for spec in style.shadows.iter().rev() {
        if spec.inset != inset || spec.is_invisible() {
            continue;
        }
        let mut shadow = if inset {
            Shadow::inset_shadow(
                placement.border_box,
                (spec.offset_x, spec.offset_y),
                spec.spread,
                spec.deviation,
                spec.color,
            )
        } else {
            Shadow::drop_shadow(
                placement.border_box,
                (spec.offset_x, spec.offset_y),
                spec.spread,
                spec.deviation,
                spec.color,
            )
        };
        // A spread grows the shape a drop shadow casts and *shrinks* the one an inset shadow leaves
        // open, so the corners of the two move in opposite directions: an inset shadow's shape is
        // concentric inside the box, and growing its radii would round the hole the wrong way.
        let spread = if inset { -spec.spread } else { spec.spread };
        shadow.radii = flatten(grow(placement.radii, spread));
        shadow.element_radii = flatten(placement.radii);
        shadow.transform = placement.transform.index();
        pushed += usize::from(scene.push_shadow(shadow.clipped(placement.clip)).is_some());
    }
    pushed
}

/// Emits a box's background and its border, which are one quad when the background is a flat colour.
///
/// A gradient layer is a quad of its own drawn over the colour, and the border rides on the last
/// quad pushed so that its stroke is drawn against the outermost fill rather than under it.
pub fn background_and_border(
    scene: &mut Scene,
    style: &PaintStyle,
    placement: BoxPlacement,
) -> usize {
    let mut pushed = 0;
    let has_color = style.background.color.alpha() != 0.0;
    let layers = &style.background.layers;
    let borders = border_widths(placement.border);
    let stroke = border_stroke(scene, style);

    // The colour first, then each layer over it, last written first — CSS paints the first layer on
    // top. The border goes on whichever quad is drawn last, so that it is not covered by a fill.
    //
    // The fills are collected before any of them is pushed, because which quad carries the border
    // is not knowable until the last fill has been resolved: a layer that resolves to no paint at
    // all is not drawn, and a loop that decided from the layer *count* would hang the border on a
    // quad that never arrives and drop it.
    let mut fills: SmallVec<[PaintRef; 2]> = SmallVec::new();
    if has_color {
        fills.push(
            scene
                .paints
                .add(zgui_scene::Paint::Solid(style.background.color)),
        );
    }
    // A ramp is resolved against the box with its corner at the origin rather than where the box
    // is standing, so what it is interned under is the box's *size* and the gradient written on
    // it. Two identical rows anywhere on the surface then share one entry, and a box carried along
    // by a scroll goes on sharing the entry it already had instead of minting one per position for
    // as long as the scroll runs. Where the box is standing is put back by the quad, which carries
    // the corner as the origin its paints are described from.
    let local = Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        placement.border_box.size,
    );
    for layer in layers.iter().rev() {
        if let Some(fill) = gradient_paint(scene, layer, local, placement.scale) {
            fills.push(fill);
        }
    }
    let draws_border = !style.border.invisible && borders.iter().any(|width| *width > 0.0);
    // A box with a border and no fill at all still draws its border, and that is the case a
    // fill-driven loop misses entirely.
    if fills.is_empty() && draws_border {
        fills.push(PaintRef::NONE);
    }
    let last = fills.len().saturating_sub(1);
    for (index, fill) in fills.into_iter().enumerate() {
        pushed += usize::from(
            push(
                scene,
                placement,
                fill,
                (index == last).then_some((borders, stroke, style)),
            )
            .is_some(),
        );
    }
    pushed
}

/// Pushes one quad of a box, optionally carrying the border.
fn push(
    scene: &mut Scene,
    placement: BoxPlacement,
    fill: PaintRef,
    border: Option<([f32; 4], PaintRef, &PaintStyle)>,
) -> Option<zgui_scene::DrawOrder> {
    let mut quad = Quad::filled(placement.border_box, fill)
        .clipped(placement.clip)
        .transformed(placement.transform)
        .with_radii(placement.radii);
    if let Some((widths, stroke, style)) = border {
        quad = quad.with_border(widths, stroke, style.border.style.to_scene());
    }
    // Only a paint that is read at a point has an origin to be read from, and saying so is what
    // keeps every flat-filled box in the document carrying a zero it does not use.
    if quad.samples_its_paint() {
        quad.reanchor_paint(Size::new(
            placement.border_box.origin.x,
            placement.border_box.origin.y,
        ));
    }
    scene.push_quad(quad)
}

/// Emits a box's outline, which is drawn outside the border box and after the box's descendants.
pub fn outline(scene: &mut Scene, style: &PaintStyle, placement: BoxPlacement) -> usize {
    let Some(outline) = &style.outline else {
        return 0;
    };
    let reach = outline.offset + outline.width;
    let rect = placement.border_box.outset(Edges {
        top: DevicePx(reach),
        right: DevicePx(reach),
        bottom: DevicePx(reach),
        left: DevicePx(reach),
    });
    if rect.is_empty() {
        return 0;
    }
    let stroke = scene.paints.add(zgui_scene::Paint::Solid(outline.color));
    let quad = Quad::filled(rect, PaintRef::NONE)
        .clipped(placement.clip)
        .transformed(placement.transform)
        .with_radii(grow(placement.radii, reach))
        .with_border([outline.width; 4], stroke, outline.style.to_scene());
    usize::from(scene.push_quad(quad).is_some())
}

/// The border widths a quad carries, in top, right, bottom, left order.
fn border_widths(border: Edges<DevicePx>) -> [f32; 4] {
    [border.top.0, border.right.0, border.bottom.0, border.left.0]
}

/// The paint a box's border is stroked with.
///
/// One quad carries one stroke, so four differently coloured sides are drawn in the first side's
/// colour that is not fully transparent — which is the top's in every stylesheet that sets a
/// shorthand, and which is visibly wrong only for the deliberately multicoloured border.
fn border_stroke(scene: &mut Scene, style: &PaintStyle) -> PaintRef {
    let color = style
        .border
        .colors
        .iter()
        .copied()
        .find(|color| color.alpha() != 0.0)
        .unwrap_or(Color::TRANSPARENT);
    if color.alpha() == 0.0 {
        return PaintRef::NONE;
    }
    scene.paints.add(zgui_scene::Paint::Solid(color))
}

/// The eight-float form of four elliptical radii.
pub fn flatten(radii: Corners<Vec2<DevicePx>>) -> [f32; 8] {
    [
        radii.top_left.x.0,
        radii.top_left.y.0,
        radii.top_right.x.0,
        radii.top_right.y.0,
        radii.bottom_right.x.0,
        radii.bottom_right.y.0,
        radii.bottom_left.x.0,
        radii.bottom_left.y.0,
    ]
}

/// Radii grown outwards by `by`, which is what an outline's and a spread shadow's corners take.
///
/// A square corner stays square however far it is grown: growing a rectangle with a sharp corner
/// does not round it.
pub fn grow(radii: Corners<Vec2<DevicePx>>, by: f32) -> Corners<Vec2<DevicePx>> {
    let out = |radius: Vec2<DevicePx>| {
        if radius.x.0 == 0.0 && radius.y.0 == 0.0 {
            return radius;
        }
        Vec2::new(
            DevicePx((radius.x.0 + by).max(0.0)),
            DevicePx((radius.y.0 + by).max(0.0)),
        )
    };
    Corners {
        top_left: out(radii.top_left),
        top_right: out(radii.top_right),
        bottom_right: out(radii.bottom_right),
        bottom_left: out(radii.bottom_left),
    }
}

/// The radii of a box's padding box, which a caller clipping content to the box needs.
pub fn padding_radii(
    radii: Corners<Vec2<DevicePx>>,
    border: Edges<DevicePx>,
) -> Corners<Vec2<DevicePx>> {
    inner_radii(radii, border)
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Corners, Device, DevicePx, Point, Rect, Size, Vec2};

    use super::{BoxPlacement, grow, inset_shadows, outer_shadows};
    use crate::lower::PaintStyle;
    use crate::lower::shadow::ShadowSpec;

    /// A 100 by 50 box with a uniform ten-pixel radius.
    fn placement() -> BoxPlacement {
        BoxPlacement {
            border_box: Rect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(100.0), DevicePx(50.0)),
            ),
            border: zgui_geom::Edges::ZERO,
            radii: Corners::uniform(Vec2::new(DevicePx(10.0), DevicePx(10.0))),
            clip: zgui_scene::ClipId::ROOT,
            transform: zgui_scene::SpatialId::VIEWPORT,
            scale: 1.0,
        }
    }

    /// The initial style carrying one shadow with the given spread and direction.
    fn with_shadow(spread: f32, inset: bool) -> PaintStyle {
        let mut style = crate::lower::lower(&zgui_css::StyleDraft::initial().build(), 1.0);
        style.shadows.push(ShadowSpec {
            offset_x: 0.0,
            offset_y: 0.0,
            deviation: 0.0,
            spread,
            color: zgui_color::Color::BLACK,
            inset,
        });
        style
    }

    /// The corner radii of the only shadow a scene holds.
    fn only_shadow_radius(style: &PaintStyle, inset: bool) -> f32 {
        let mut scene = zgui_scene::Scene::new();
        scene.begin_frame(Size::<i32, Device>::new(256, 256));
        let pushed = if inset {
            inset_shadows(&mut scene, style, placement())
        } else {
            outer_shadows(&mut scene, style, placement())
        };
        assert_eq!(pushed, 1, "the fixture has to push exactly one shadow");
        scene.primitives.shadows[0].radii[0]
    }

    #[test]
    fn a_spread_grows_a_drop_shadows_corners_and_shrinks_an_inset_ones() {
        // The two shapes move in opposite directions: a drop shadow's shape is the box grown by the
        // spread, an inset shadow's is the box shrunk by it, and a corner stays concentric with the
        // edge it belongs to. One sign for both would round the hole an inset shadow leaves the
        // wrong way, by twice the spread.
        assert_eq!(only_shadow_radius(&with_shadow(6.0, false), false), 16.0);
        assert_eq!(only_shadow_radius(&with_shadow(6.0, true), true), 4.0);
    }

    #[test]
    fn a_square_corner_stays_square_however_far_it_grows() {
        let radii = Corners {
            top_left: Vec2::new(DevicePx(0.0), DevicePx(0.0)),
            top_right: Vec2::new(DevicePx(4.0), DevicePx(8.0)),
            bottom_right: Vec2::new(DevicePx(0.0), DevicePx(0.0)),
            bottom_left: Vec2::new(DevicePx(0.0), DevicePx(0.0)),
        };
        let grown = grow(radii, 6.0);
        assert_eq!(grown.top_left, Vec2::new(DevicePx(0.0), DevicePx(0.0)));
        assert_eq!(grown.top_right, Vec2::new(DevicePx(10.0), DevicePx(14.0)));
    }

    #[test]
    fn a_negative_spread_never_makes_a_radius_negative() {
        let radii = Corners::uniform(Vec2::new(DevicePx(2.0), DevicePx(2.0)));
        let shrunk = grow(radii, -10.0);
        assert_eq!(shrunk.top_left, Vec2::new(DevicePx(0.0), DevicePx(0.0)));
    }
}
