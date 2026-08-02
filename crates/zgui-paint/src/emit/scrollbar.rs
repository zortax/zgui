//! Scrollbars: the groove and the thumb.
//!
//! Both are quads, and what separates them from any other quad is where they sit: a scrollbar is
//! drawn *outside* the scrollport it belongs to and is not clipped by it, so its geometry comes from
//! the scroll region rather than from the box's content.
//!
//! The rectangles the layout hands over span the gutter's full breadth, and they must: they are
//! also what the hit index answers presses with, and a strip a pointer can miss by two pixels is a
//! bar nobody can grab. The *paint* is under no such obligation, which is why the thumb drawn here
//! is slimmer than the rectangle it was given — the modern overlay look, without giving up any of
//! the grab area.

use zgui_color::Color;
use zgui_geom::{Corners, Device, DevicePx, Point, Rect, Size, Vec2};
use zgui_layout::fragment::ScrollbarPart;
use zgui_scene::{ClipId, Paint, Quad, Scene, SpatialId};

/// How a scrollbar's two parts are painted.
///
/// The track is transparent in every default: presses in the groove still page, because the hit
/// index answers from the layout's fragments and never from what was drawn, so the groove earns
/// its keep invisibly. The thumb alone says where the content sits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarPaint {
    /// The groove's colour.
    pub track: Color,
    /// The thumb's colour.
    pub thumb: Color,
    /// How thick the painted thumb is across the gutter, in device pixels.
    ///
    /// The thumb's rectangle keeps the gutter's full breadth for hitting; only this much of it is
    /// filled, centred, so the bar reads as a slim floating capsule rather than a plank.
    pub thumb_thickness: f32,
}

impl ScrollbarPaint {
    /// The colour one part is drawn in.
    pub fn color(&self, part: ScrollbarPart) -> Color {
        match part {
            ScrollbarPart::Track => self.track,
            ScrollbarPart::Thumb => self.thumb,
        }
    }

    /// The corner radii one part is drawn with.
    ///
    /// A groove is a rectangle and a thumb is a capsule with fully round ends — the radius is half
    /// of whatever the painted rectangle's narrow side turned out to be, so the ends stay
    /// semicircular however slim the bar is drawn.
    pub fn radii(
        &self,
        part: ScrollbarPart,
        bounds: Rect<DevicePx, Device>,
    ) -> Corners<Vec2<DevicePx>> {
        match part {
            ScrollbarPart::Track => Corners::uniform(Vec2::splat(DevicePx(0.0))),
            ScrollbarPart::Thumb => {
                let radius = (bounds.size.width.0.min(bounds.size.height.0) * 0.5).max(0.0);
                Corners::uniform(Vec2::splat(DevicePx(radius)))
            }
        }
    }
}

/// The slim bar painted inside a thumb's reserved rectangle.
///
/// The narrow side of the rectangle is always its breadth: the layout never lets a thumb shrink
/// below [`MIN_THUMB`](zgui_layout::scroll_region::bar::travel::MIN_THUMB) along its travel, which
/// exceeds the theme's gutter width, so shrinking the shorter side is shrinking the right one. The
/// length is left alone — a thumb that got shorter as well would lie about how much is visible.
fn slim(bounds: Rect<DevicePx, Device>, thickness: f32) -> Rect<DevicePx, Device> {
    let Size { width, height, .. } = bounds.size;
    if width.0 <= height.0 {
        let painted = thickness.clamp(0.0, width.0);
        let inset = (width.0 - painted) * 0.5;
        Rect::new(
            Point::new(DevicePx(bounds.left().0 + inset), bounds.origin.y),
            Size::new(DevicePx(painted), height),
        )
    } else {
        let painted = thickness.clamp(0.0, height.0);
        let inset = (height.0 - painted) * 0.5;
        Rect::new(
            Point::new(bounds.origin.x, DevicePx(bounds.top().0 + inset)),
            Size::new(width, DevicePx(painted)),
        )
    }
}

/// Emits one part of one scrollbar, and returns how many primitives were pushed.
///
/// A scrollbar is drawn through the chain of whatever clips the *scroller*, never through the
/// scrollport's own clip: a bar clipped by the region it scrolls would disappear the moment the
/// content did.
pub fn emit(
    scene: &mut Scene,
    part: ScrollbarPart,
    bounds: Rect<DevicePx, Device>,
    paint: ScrollbarPaint,
    clip: ClipId,
) -> usize {
    let color = paint.color(part);
    let bounds = match part {
        ScrollbarPart::Track => bounds,
        ScrollbarPart::Thumb => slim(bounds, paint.thumb_thickness),
    };
    if bounds.is_empty() || color.alpha() == 0.0 {
        return 0;
    }
    let fill = scene.paints.add(Paint::Solid(color));
    let quad = Quad::filled(bounds, fill)
        .clipped(clip)
        .transformed(SpatialId::VIEWPORT)
        .with_radii(paint.radii(part, bounds));
    usize::from(scene.push_quad(quad).is_some())
}

/// The paint a scrollbar takes when nothing has themed it.
///
/// The thumb is deliberately visible rather than faint — a scroller with no bar at all reads as
/// content that cannot be scrolled — while the track paints nothing. The groove's rectangle still
/// exists and still catches presses; drawing it grey would only fence off a strip of the page for
/// a control that the thumb alone already announces.
pub fn default_paint() -> ScrollbarPaint {
    ScrollbarPaint {
        track: Color::TRANSPARENT,
        thumb: Color::srgb(0.0, 0.0, 0.0, 0.35),
        thumb_thickness: 6.0,
    }
}

/// The same, for a window presented in a dark scheme.
///
/// The light paint is translucent black, which on a dark background is a bar darker than the page
/// it sits beside — technically drawn, and invisible. Inverting it is what every platform does, and
/// the alpha is markedly higher because a light film over a dark surface reads as far fainter than
/// a dark film of the same strength over a light one.
pub fn dark_paint() -> ScrollbarPaint {
    ScrollbarPaint {
        track: Color::TRANSPARENT,
        thumb: Color::srgb(1.0, 1.0, 1.0, 0.5),
        thumb_thickness: 6.0,
    }
}

/// The paint for a window presented in `dark` or not.
pub fn paint_for(dark: bool) -> ScrollbarPaint {
    if dark { dark_paint() } else { default_paint() }
}

/// A fully transparent paint, which draws no scrollbar at all.
pub fn hidden_paint() -> ScrollbarPaint {
    ScrollbarPaint {
        track: Color::TRANSPARENT,
        thumb: Color::TRANSPARENT,
        thumb_thickness: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};
    use zgui_layout::fragment::ScrollbarPart;
    use zgui_scene::{ClipId, Scene};

    use super::{default_paint, emit, hidden_paint, slim};

    /// A vertical bar twelve pixels wide.
    fn bar() -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(188.0), DevicePx(0.0)),
            Size::new(DevicePx(12.0), DevicePx(200.0)),
        )
    }

    #[test]
    fn a_thumb_is_a_capsule_and_a_track_is_a_rectangle() {
        let paint = default_paint();
        let thumb = paint.radii(ScrollbarPart::Thumb, slim(bar(), paint.thumb_thickness));
        let track = paint.radii(ScrollbarPart::Track, bar());
        assert_eq!(thumb.top_left.x, DevicePx(3.0), "half the painted thickness");
        assert_eq!(track.top_left.x, DevicePx(0.0));
    }

    #[test]
    fn a_thumbs_radius_never_exceeds_half_its_narrow_side() {
        let narrow = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(3.0), DevicePx(200.0)),
        );
        let radii = default_paint().radii(ScrollbarPart::Thumb, narrow);
        assert_eq!(radii.top_left.x, DevicePx(1.5));
    }

    #[test]
    fn the_painted_thumb_is_a_centred_sliver_of_the_gutter_it_can_be_grabbed_in() {
        let vertical = slim(bar(), 6.0);
        assert_eq!(vertical.left(), DevicePx(191.0), "three pixels in from each edge");
        assert_eq!(vertical.size.width, DevicePx(6.0));
        assert_eq!(vertical.top(), bar().top(), "the length is untouched");
        assert_eq!(vertical.size.height, bar().size.height);

        let horizontal = slim(
            Rect::new(
                Point::new(DevicePx(0.0), DevicePx(188.0)),
                Size::new(DevicePx(200.0), DevicePx(12.0)),
            ),
            6.0,
        );
        assert_eq!(horizontal.top(), DevicePx(191.0));
        assert_eq!(horizontal.size.height, DevicePx(6.0));
        assert_eq!(horizontal.size.width, DevicePx(200.0));
    }

    #[test]
    fn a_thickness_wider_than_the_gutter_paints_the_gutter_and_no_more() {
        let full = slim(bar(), 40.0);
        assert_eq!(full, bar(), "the paint never spills past the geometry");
    }

    #[test]
    fn the_thumb_reaches_the_display_list_and_the_transparent_track_does_not() {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(200, 200));
        let paint = default_paint();
        assert_eq!(
            emit(&mut scene, ScrollbarPart::Track, bar(), paint, ClipId::ROOT),
            0,
            "an unpainted groove still pages, from geometry the hit index keeps"
        );
        assert_eq!(
            emit(&mut scene, ScrollbarPart::Thumb, bar(), paint, ClipId::ROOT),
            1
        );
        assert_eq!(scene.primitives.quads.len(), 1);
        let quad = &scene.primitives.quads[0];
        assert_eq!(
            quad.bounds,
            [191.0, 0.0, 6.0, 200.0],
            "the quad is the slimmed rectangle, not the reserved one"
        );

        let hidden = hidden_paint();
        assert_eq!(
            emit(
                &mut scene,
                ScrollbarPart::Track,
                bar(),
                hidden,
                ClipId::ROOT
            ),
            0
        );
        assert_eq!(
            emit(
                &mut scene,
                ScrollbarPart::Thumb,
                bar(),
                hidden,
                ClipId::ROOT
            ),
            0
        );
        assert_eq!(scene.primitives.quads.len(), 1, "nothing more was pushed");
    }
}
