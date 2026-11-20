//! The parts of a surface that take input or are opaque, in the compositor's whole pixels.
//!
//! A region is a set of rectangles in surface-local logical pixels, which is the space a CSS pixel
//! is drawn in here. The compositor measures them in whole pixels, so a rectangle that a
//! fractional scale left between pixels is rounded: outwards for input, so no pixel that shows a
//! control lets a press through, and inwards for opacity, so no pixel that is partly transparent
//! hides what is behind it.

use zgui_geom::{Css, CssPx, Rect};

/// A rectangle in whole logical pixels: left, top, width, height.
pub type Whole = (i32, i32, i32, i32);

/// What a surface was asked to change about one of its regions, and has not sent yet.
///
/// The outer absence is "nothing to send". The inner one is the region the protocol spells as no
/// region at all: the whole surface for input, nothing for opacity.
pub type Owed = Option<Option<Vec<Rect<CssPx, Css>>>>;

/// `rect` grown to the whole pixels it touches.
pub fn outward(rect: Rect<CssPx, Css>) -> Option<Whole> {
    whole(
        rect.left().0.floor(),
        rect.top().0.floor(),
        rect.right().0.ceil(),
        rect.bottom().0.ceil(),
    )
}

/// `rect` shrunk to the whole pixels it covers completely.
pub fn inward(rect: Rect<CssPx, Css>) -> Option<Whole> {
    whole(
        rect.left().0.ceil(),
        rect.top().0.ceil(),
        rect.right().0.floor(),
        rect.bottom().0.floor(),
    )
}

/// The rectangle between two corners, when it covers any pixel at all.
fn whole(left: f32, top: f32, right: f32, bottom: f32) -> Option<Whole> {
    let finite = [left, top, right, bottom]
        .iter()
        .all(|edge| edge.is_finite());
    if !finite || right <= left || bottom <= top {
        return None;
    }
    let clamp = |value: f32| value.clamp(i32::MIN as f32, i32::MAX as f32) as i32;
    let (left, top) = (clamp(left), clamp(top));
    Some((
        left,
        top,
        clamp(right).saturating_sub(left),
        clamp(bottom).saturating_sub(top),
    ))
}

#[cfg(test)]
mod tests {
    use super::{inward, outward};
    use zgui_geom::{CssPx, Point, Rect, Size};

    fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<CssPx, zgui_geom::Css> {
        Rect::new(
            Point::new(CssPx(x), CssPx(y)),
            Size::new(CssPx(width), CssPx(height)),
        )
    }

    #[test]
    fn a_rectangle_on_whole_pixels_is_sent_as_it_is() {
        assert_eq!(
            outward(rect(8.0, 4.0, 320.0, 200.0)),
            Some((8, 4, 320, 200))
        );
        assert_eq!(inward(rect(8.0, 4.0, 320.0, 200.0)), Some((8, 4, 320, 200)));
    }

    #[test]
    fn input_grows_to_every_pixel_it_touches() {
        // A card at 1.25 scale whose border box is 101 device pixels from the left edge.
        assert_eq!(
            outward(rect(80.8, 0.4, 100.0, 10.0)),
            Some((80, 0, 101, 11))
        );
    }

    #[test]
    fn opacity_shrinks_to_the_pixels_it_covers() {
        assert_eq!(inward(rect(80.8, 0.4, 100.0, 10.0)), Some((81, 1, 99, 9)));
    }

    #[test]
    fn a_rectangle_with_no_pixel_left_is_dropped() {
        assert_eq!(inward(rect(0.2, 0.2, 0.6, 5.0)), None);
        assert_eq!(outward(rect(3.0, 3.0, 0.0, 5.0)), None);
        assert_eq!(outward(rect(f32::NAN, 0.0, 5.0, 5.0)), None);
    }
}
