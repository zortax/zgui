//! Where a surface of a known size actually goes, given an anchor and a window to stay inside.
//!
//! Pure arithmetic over four rectangles, so every case that matters — the anchor at each edge, a
//! surface taller than the window, a window smaller than the surface — is a unit test rather than
//! something to be seen by opening a menu near the bottom of a screen and squinting.

use zgui::geom::{Device, DevicePx, Point, Rect, Size};

use crate::popper::placement::{Align, Placement, Side};

/// A rectangle in the window's own pixels.
pub type WindowRect = Rect<DevicePx, Device>;

/// What a [`Popper`](crate::Popper) is asked to do.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct PopperOptions {
    /// Where the surface is asked to go.
    pub placement: Placement,
    /// Whether to cross to the other side of the anchor when there is not enough room.
    pub flip: bool,
    /// Whether to slide along the anchor's edge to stay inside the window.
    pub shift: bool,
    /// How far off the anchor the surface sits, in pixels.
    pub offset: f32,
    /// How close to the window's edge the surface may come, in pixels.
    pub padding: f32,
}

impl Default for PopperOptions {
    fn default() -> Self {
        Self {
            placement: Placement::BOTTOM,
            flip: true,
            shift: true,
            offset: 4.0,
            padding: 8.0,
        }
    }
}

/// Where the surface goes, and where it ended up going.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Solution {
    /// The surface's top-left corner, in window pixels.
    pub origin: Point<DevicePx, Device>,
    /// Where it actually went, which may not be where it was asked to go.
    pub placement: Placement,
    /// How far the surface still hangs outside the window after everything was tried.
    ///
    /// Zero in every case that fits. Non-zero means the window is genuinely too small, and a
    /// caller that wants to clamp the surface's own size reads this rather than measuring again.
    pub overflow: f32,
}

/// Places `floating` against `anchor`, inside `viewport`.
///
/// Three steps, in this order, because each depends on the last:
///
/// 1. **place** the surface on the side and alignment asked for;
/// 2. **flip** to the opposite side when that side has less room than the other — and only when
///    the other side has *enough*, so a surface that fits nowhere stays where it was asked rather
///    than flapping between two equally bad choices;
/// 3. **shift** along the anchor's edge until the surface is inside the window, which is what
///    keeps a menu under a button at the far right of the window on screen.
///
/// ```
/// use zgui::geom::{DevicePx, Point, Rect, Size};
/// use zgui_ui_primitives::popper::{PopperOptions, Side, solve};
///
/// let window = Rect::new(
///     Point::new(DevicePx(0.0), DevicePx(0.0)),
///     Size::new(DevicePx(800.0), DevicePx(600.0)),
/// );
/// // A trigger right at the bottom of the window …
/// let anchor = Rect::new(
///     Point::new(DevicePx(100.0), DevicePx(560.0)),
///     Size::new(DevicePx(80.0), DevicePx(24.0)),
/// );
/// let surface = Size::new(DevicePx(200.0), DevicePx(160.0));
///
/// // … and the menu goes above it rather than off the bottom.
/// let solved = solve(anchor, surface, window, &PopperOptions::default());
/// assert_eq!(solved.placement.side, Side::Top);
/// assert!(solved.origin.y.0 >= 0.0);
/// ```
pub fn solve(
    anchor: WindowRect,
    floating: Size<DevicePx, Device>,
    viewport: WindowRect,
    options: &PopperOptions,
) -> Solution {
    let mut placement = options.placement;

    if options.flip {
        let asked = room_on(placement.side, anchor, viewport);
        let across = room_on(placement.side.opposite(), anchor, viewport);
        let needed = extent(placement.side, floating) + options.offset;
        // Only when the other side actually fits. Flipping to a side that is merely *less* short
        // is how a surface in a window too small for it oscillates every frame.
        if asked < needed && across >= needed {
            placement = placement.flipped();
        }
    }

    let mut origin = place(placement, anchor, floating, options.offset);
    if options.shift {
        origin = shift_into(origin, floating, viewport, placement.side, options.padding);
    }

    Solution {
        origin,
        placement,
        overflow: overflow_of(origin, floating, viewport),
    }
}

/// How much room there is between the anchor and the window's edge on `side`.
fn room_on(side: Side, anchor: WindowRect, viewport: WindowRect) -> f32 {
    match side {
        Side::Top => anchor.origin.y.0 - viewport.origin.y.0,
        Side::Bottom => {
            (viewport.origin.y.0 + viewport.size.height.0)
                - (anchor.origin.y.0 + anchor.size.height.0)
        }
        Side::Left => anchor.origin.x.0 - viewport.origin.x.0,
        Side::Right => {
            (viewport.origin.x.0 + viewport.size.width.0)
                - (anchor.origin.x.0 + anchor.size.width.0)
        }
    }
}

/// How much of the surface has to fit along `side`'s axis.
fn extent(side: Side, floating: Size<DevicePx, Device>) -> f32 {
    if side.is_vertical() {
        floating.height.0
    } else {
        floating.width.0
    }
}

/// The surface's corner for a placement, before anything is shifted.
fn place(
    placement: Placement,
    anchor: WindowRect,
    floating: Size<DevicePx, Device>,
    offset: f32,
) -> Point<DevicePx, Device> {
    let (anchor_x, anchor_y) = (anchor.origin.x.0, anchor.origin.y.0);
    let (anchor_w, anchor_h) = (anchor.size.width.0, anchor.size.height.0);
    let (width, height) = (floating.width.0, floating.height.0);

    let (x, y) = match placement.side {
        Side::Top => (
            along(placement.align, anchor_x, anchor_w, width),
            anchor_y - height - offset,
        ),
        Side::Bottom => (
            along(placement.align, anchor_x, anchor_w, width),
            anchor_y + anchor_h + offset,
        ),
        Side::Left => (
            anchor_x - width - offset,
            along(placement.align, anchor_y, anchor_h, height),
        ),
        Side::Right => (
            anchor_x + anchor_w + offset,
            along(placement.align, anchor_y, anchor_h, height),
        ),
    };
    Point::new(DevicePx(x), DevicePx(y))
}

/// Where the surface starts along the anchor's edge, for one alignment.
fn along(align: Align, anchor_start: f32, anchor_extent: f32, extent: f32) -> f32 {
    match align {
        Align::Start => anchor_start,
        Align::Center => anchor_start + (anchor_extent - extent) / 2.0,
        Align::End => anchor_start + anchor_extent - extent,
    }
}

/// Slides the surface along the anchor's edge until it is inside the window.
///
/// Only along the *cross* axis: moving it along the side's own axis would take it off the anchor,
/// which is what flipping is for.
fn shift_into(
    origin: Point<DevicePx, Device>,
    floating: Size<DevicePx, Device>,
    viewport: WindowRect,
    side: Side,
    padding: f32,
) -> Point<DevicePx, Device> {
    let (mut x, mut y) = (origin.x.0, origin.y.0);
    if side.is_vertical() {
        x = clamp_within(
            x,
            floating.width.0,
            viewport.origin.x.0,
            viewport.size.width.0,
            padding,
        );
    } else {
        y = clamp_within(
            y,
            floating.height.0,
            viewport.origin.y.0,
            viewport.size.height.0,
            padding,
        );
    }
    Point::new(DevicePx(x), DevicePx(y))
}

/// Clamps a span of `extent` into `[start + padding, start + available - padding]`.
///
/// A surface wider than the room available is pinned to the leading edge rather than centred on
/// its own overflow, so what is cut off is the end of it and the beginning stays readable.
fn clamp_within(position: f32, extent: f32, start: f32, available: f32, padding: f32) -> f32 {
    let low = start + padding;
    let high = start + available - padding - extent;
    if high < low {
        low
    } else {
        position.clamp(low, high)
    }
}

/// How far the surface hangs outside the window, on its worst edge.
fn overflow_of(
    origin: Point<DevicePx, Device>,
    floating: Size<DevicePx, Device>,
    viewport: WindowRect,
) -> f32 {
    let left = viewport.origin.x.0 - origin.x.0;
    let top = viewport.origin.y.0 - origin.y.0;
    let right = (origin.x.0 + floating.width.0) - (viewport.origin.x.0 + viewport.size.width.0);
    let bottom = (origin.y.0 + floating.height.0) - (viewport.origin.y.0 + viewport.size.height.0);
    [left, top, right, bottom]
        .into_iter()
        .fold(0.0_f32, f32::max)
}

#[cfg(test)]
mod tests {
    use zgui::geom::{DevicePx, Point, Rect, Size};

    use super::{PopperOptions, Solution, WindowRect, solve};
    use crate::popper::placement::{Align, Placement, Side};

    fn rect(x: f32, y: f32, width: f32, height: f32) -> WindowRect {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    fn size(width: f32, height: f32) -> Size<DevicePx, Device> {
        Size::new(DevicePx(width), DevicePx(height))
    }

    use zgui::geom::Device;

    fn window() -> WindowRect {
        rect(0.0, 0.0, 800.0, 600.0)
    }

    fn solved(anchor: WindowRect, floating: Size<DevicePx, Device>) -> Solution {
        solve(anchor, floating, window(), &PopperOptions::default())
    }

    #[test]
    fn a_surface_with_room_goes_exactly_where_it_was_asked() {
        let anchor = rect(300.0, 200.0, 100.0, 30.0);
        let solution = solved(anchor, size(200.0, 120.0));
        assert_eq!(solution.placement, Placement::BOTTOM);
        // Centred on the anchor, four pixels below it.
        assert_eq!(solution.origin.x.0, 300.0 + (100.0 - 200.0) / 2.0);
        assert_eq!(solution.origin.y.0, 200.0 + 30.0 + 4.0);
        assert_eq!(solution.overflow, 0.0);
    }

    #[test]
    fn a_surface_with_no_room_below_goes_above_and_stays_inside() {
        let anchor = rect(100.0, 560.0, 80.0, 24.0);
        let solution = solved(anchor, size(200.0, 160.0));
        assert_eq!(solution.placement.side, Side::Top);
        assert_eq!(solution.origin.y.0, 560.0 - 160.0 - 4.0);
        assert_eq!(solution.overflow, 0.0);
    }

    #[test]
    fn a_surface_that_fits_nowhere_stays_where_it_was_asked_rather_than_flapping() {
        // The oscillation this prevents has no still frame to catch it in: each frame the surface
        // flips to the side with more room, which is then the side with less.
        let anchor = rect(100.0, 300.0, 80.0, 24.0);
        let taller_than_the_window = size(200.0, 700.0);
        let first = solved(anchor, taller_than_the_window);
        assert_eq!(first.placement, Placement::BOTTOM, "it stayed put");
        assert!(first.overflow > 0.0, "and said how badly it does not fit");

        // Solving again from the answer gives the same answer, which is the whole property.
        let second = solved(anchor, taller_than_the_window);
        assert_eq!(first, second);
    }

    #[test]
    fn a_surface_at_the_right_edge_slides_back_in_without_leaving_the_anchor() {
        let anchor = rect(760.0, 100.0, 30.0, 30.0);
        let solution = solved(anchor, size(240.0, 100.0));
        assert_eq!(solution.placement.side, Side::Bottom, "it did not flip");
        assert_eq!(
            solution.origin.x.0,
            800.0 - 8.0 - 240.0,
            "it slid in to the padding"
        );
        assert_eq!(solution.overflow, 0.0);
    }

    #[test]
    fn a_surface_at_the_left_edge_slides_the_other_way() {
        let anchor = rect(4.0, 100.0, 30.0, 30.0);
        let solution = solved(anchor, size(240.0, 100.0));
        assert_eq!(solution.origin.x.0, 8.0);
    }

    #[test]
    fn shifting_moves_along_the_edge_and_never_across_it() {
        // Sliding along the side's own axis would take the surface off its anchor, which is the
        // one thing shifting must never do — that is what flipping is for.
        let anchor = rect(760.0, 100.0, 30.0, 30.0);
        let solution = solved(anchor, size(240.0, 100.0));
        assert_eq!(
            solution.origin.y.0,
            100.0 + 30.0 + 4.0,
            "the offset from the anchor is untouched"
        );
    }

    #[test]
    fn each_alignment_lines_the_surface_up_where_it_says() {
        let anchor = rect(300.0, 200.0, 100.0, 30.0);
        let floating = size(60.0, 40.0);
        let at = |align| {
            solve(
                anchor,
                floating,
                window(),
                &PopperOptions {
                    placement: Placement::new(Side::Bottom, align),
                    ..PopperOptions::default()
                },
            )
            .origin
            .x
            .0
        };
        assert_eq!(at(Align::Start), 300.0);
        assert_eq!(at(Align::Center), 300.0 + (100.0 - 60.0) / 2.0);
        assert_eq!(at(Align::End), 300.0 + 100.0 - 60.0);
    }

    #[test]
    fn a_side_placement_flips_across_the_horizontal_axis() {
        let anchor = rect(700.0, 200.0, 80.0, 30.0);
        let solution = solve(
            anchor,
            size(200.0, 100.0),
            window(),
            &PopperOptions {
                placement: Placement::new(Side::Right, Align::Center),
                ..PopperOptions::default()
            },
        );
        assert_eq!(solution.placement.side, Side::Left);
        assert_eq!(solution.origin.x.0, 700.0 - 200.0 - 4.0);
    }

    #[test]
    fn refusing_to_flip_leaves_the_surface_hanging_and_says_so() {
        let anchor = rect(100.0, 560.0, 80.0, 24.0);
        let solution = solve(
            anchor,
            size(200.0, 160.0),
            window(),
            &PopperOptions {
                flip: false,
                ..PopperOptions::default()
            },
        );
        assert_eq!(solution.placement, Placement::BOTTOM);
        assert!(solution.overflow > 0.0);
    }

    #[test]
    fn refusing_to_shift_leaves_it_where_the_alignment_put_it() {
        let anchor = rect(760.0, 100.0, 30.0, 30.0);
        let solution = solve(
            anchor,
            size(240.0, 100.0),
            window(),
            &PopperOptions {
                shift: false,
                ..PopperOptions::default()
            },
        );
        assert_eq!(solution.origin.x.0, 760.0 + (30.0 - 240.0) / 2.0);
    }

    #[test]
    fn a_window_that_does_not_start_at_the_origin_is_respected() {
        // The window rectangle is whatever the engine measured, and nothing here may assume it
        // starts at zero: an inset surface, a second monitor, a test that offsets everything.
        let viewport = rect(100.0, 50.0, 400.0, 300.0);
        let anchor = rect(120.0, 320.0, 40.0, 20.0);
        let solution = solve(
            anchor,
            size(200.0, 120.0),
            viewport,
            &PopperOptions::default(),
        );
        assert_eq!(solution.placement.side, Side::Top);
        assert!(solution.origin.y.0 >= 50.0);
        assert!(solution.origin.x.0 >= 108.0);
    }
}
