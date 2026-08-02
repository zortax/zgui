//! What a graphics device actually put on the screen for an icon.
//!
//! Every other assertion about a drawing in this framework stops at the display list. A display
//! list is satisfied by a rasteriser that writes nothing, by a composite that lands in the wrong
//! pixels, by a fill rule that closes a counter, and by a partial repaint that quietly redraws
//! everything — so each of those is asked here, of the bytes copied back off a real device.

mod raster;

use raster::measure::{self, Level};
use raster::page::{self, INK, LID, LID_TOP, PANEL};
use raster::script::MARKER;

/// The runs, or nothing on a machine with no graphics device.
macro_rules! runs {
    () => {
        match page::runs() {
            Some(runs) => runs,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// Where the drawing's ink has to land, in device pixels.
///
/// The outline is written between 3 and 21 in its own twenty-four unit square — an outer circle of
/// radius nine about the centre — so a ninety-six pixel box scales it by four and a sixteen pixel
/// padding moves it. Stated as arithmetic rather than read back from the layout tree, because a
/// rectangle read from the thing under test is a rectangle that agrees with itself.
fn expected_ink() -> (i32, i32, i32, i32) {
    let scale = page::factor();
    let origin = f64::from(page::PADDING);
    let left = (origin + 3.0 * scale).round() as i32;
    let right = (origin + 21.0 * scale).round() as i32;
    (left, left, right, right)
}

/// Whether `(x, y)` lies in the counter of the ring, clear of the glyph inside it.
///
/// The ring is an outer circle of radius nine and an inner one of radius seven, so everything
/// inside seven units of the centre is a hole — except the letter, which occupies the two units
/// either side of the middle and is excluded generously.
fn in_counter(x: i32, y: i32) -> bool {
    let scale = page::factor();
    let centre = page::centre();
    let glyph = (centre.0 - 3.0 * scale, centre.0 + 3.0 * scale);
    measure::radius(centre, x, y) <= 6.0 * scale
        && (f64::from(x) < glyph.0 || f64::from(x) > glyph.1)
}

/// Whether `(x, y)` lies well inside the ring's band, clear of either of its edges.
fn in_band(x: i32, y: i32) -> bool {
    let scale = page::factor();
    let distance = measure::radius(page::centre(), x, y);
    distance >= 7.4 * scale && distance <= 8.6 * scale
}

/// An icon in a window puts ink on the device, in the colour the text around it is in.
///
/// The colour is the whole of the claim about inheritance: nothing on the element declares a fill,
/// and `currentColor` is the default, so ink in this colour cannot have come from anywhere but the
/// cascaded `color` of the box the drawing sits in.
#[test]
fn a_mounted_icon_puts_ink_on_the_device_in_the_colour_of_the_text_around_it() {
    let runs = runs!();
    let frame = page::Runs::settled(&runs.marked);
    assert!(frame.items > 0, "the display list held no drawing at all");
    assert!(
        frame.vector_passes > 0,
        "the frame planned no rasterisation pass for a display list with {} drawings in it",
        frame.items
    );

    let covered = measure::count(&frame.full, in_band);
    assert!(
        covered > 400,
        "the ring's band is only {covered} pixels wide in this rendering, which is not a ring"
    );
    if let Some((x, y, found)) = measure::first_unlike(&frame.full, in_band, INK) {
        panic!(
            "({x}, {y}) is inside the ring and came back {found:?} rather than the {INK:?} the \
             text around the icon is in"
        );
    }
}

/// The ink lands where the box and the view box put it, and nowhere else.
///
/// This is the view box and the declared size in one measurement. Without the view box the outline
/// would be drawn at its own numbers — a twenty-four unit mark in a ninety-six pixel box — and
/// without the size from the sheet there would be no box to fit it to.
#[test]
fn the_ink_covers_exactly_the_square_the_box_and_the_view_box_put_it_in() {
    let runs = runs!();
    let frame = page::Runs::settled(&runs.marked);
    let bounds = measure::ink_bounds(&frame.full, PANEL)
        .expect("a rendering with nothing but the panel colour in it has no icon in it");
    let (left, top, right, bottom) = expected_ink();

    let slack = 1;
    assert!(
        (bounds.left() - left).abs() <= slack
            && (bounds.top() - top).abs() <= slack
            && (bounds.right() - right).abs() <= slack
            && (bounds.bottom() - bottom).abs() <= slack,
        "the ink covers {bounds:?}, not the ({left}, {top})-({right}, {bottom}) its own square \
         fitted into its own box comes to"
    );
    assert!(
        right - left > 8,
        "the expected rectangle is {}px across, so the comparison above compared two nothings",
        right - left
    );

    // And the display list said the same thing, so the device did not merely agree with itself.
    let declared = frame.declared.expect("a drawing was planned");
    assert!(
        (declared.left() - left).abs() <= slack && (declared.right() - right).abs() <= slack,
        "the display list claimed {declared:?} for a drawing the sheet puts at \
         ({left}, {top})-({right}, {bottom})"
    );
}

/// The hole in the middle of the ring shows the panel behind it.
///
/// The one assertion a bounding box cannot make. The ring is one outline: an outer circle wound one
/// way and an inner one wound the other, filled by the non-zero rule. A rasteriser that ignored the
/// winding, or a paint stage that split the two contours into separate filled shapes, produces a
/// disc — same extent, same colour, same everything a display-list assertion can reach.
#[test]
fn the_counter_of_the_ring_shows_the_panel_through_it() {
    let runs = runs!();
    let frame = page::Runs::settled(&runs.marked);

    let inside = measure::count(&frame.full, in_counter);
    assert!(
        inside > 600,
        "only {inside} pixels were tested as the ring's counter, which is too few for the hole in \
         a ninety-six pixel ring — the region is wrong, not the rendering"
    );
    if let Some((x, y, found)) = measure::first_unlike(&frame.full, in_counter, PANEL) {
        panic!(
            "({x}, {y}) is inside the ring's counter and came back {found:?} rather than the \
             panel behind it: the hole is filled in"
        );
    }
}

/// The outline's edges carry partial coverage, and only along the outline.
///
/// Two failures at once. A rasteriser with no antialiasing produces an outline of exactly two
/// colours, so there are no intermediate pixels at all; one whose output is smeared — a composite
/// sampling the scratch with the wrong filter, or at the wrong offset — produces intermediate
/// pixels far from any edge. So both the number of them and where they are is asserted.
#[test]
fn the_edges_are_shaded_rather_than_stepped_and_the_shading_is_only_at_the_edges() {
    let runs = runs!();
    let frame = page::Runs::settled(&runs.marked);
    let shaded = measure::partial(&frame.full, INK, PANEL);
    assert!(
        shaded.len() > 200,
        "only {} pixels of this rendering are a partial coverage of the ink over the panel, which \
         is what a hard-stepped outline looks like",
        shaded.len()
    );
    let levels = measure::distinct(&frame.full, &shaded, 2);
    assert!(
        levels >= 16,
        "the shaded pixels take only {levels} distinct values on the blue channel, so the edges \
         are quantised rather than antialiased"
    );

    // Every shaded pixel is on one of the two circles the ring is bounded by, or on the letter
    // inside it, which is the only other outline in this drawing.
    let scale = page::factor();
    let centre = page::centre();
    let glyph = (
        centre.0 - 2.5 * scale,
        centre.0 + 2.5 * scale,
        centre.1 - 8.0 * scale,
        centre.1 + 8.0 * scale,
    );
    let stray: Vec<_> = shaded
        .iter()
        .copied()
        .filter(|&(x, y)| {
            let distance = measure::radius(centre, x, y);
            let on_a_circle =
                (distance - 9.0 * scale).abs() <= 1.5 || (distance - 7.0 * scale).abs() <= 1.5;
            let on_the_letter = f64::from(x) >= glyph.0 - 1.5
                && f64::from(x) <= glyph.1 + 1.5
                && f64::from(y) >= glyph.2
                && f64::from(y) <= glyph.3;
            !on_a_circle && !on_the_letter
        })
        .collect();
    assert!(
        stray.len() * 50 < shaded.len(),
        "{} of {} shaded pixels lie away from any edge of this outline, starting at {:?} — that \
         is a blur, not antialiasing",
        stray.len(),
        shaded.len(),
        stray.first()
    );
}

/// A box drawn after the drawing covers it, and the panel drawn before it shows through the hole.
///
/// Submission order is z-order here — there is no depth buffer, no stencil and no order-independent
/// scheme — so a composited vector pass has to be inserted at the drawing's own index in the batch
/// stream. Inserted at the end it would cover the box; inserted at the start it would be covered by
/// the panel and there would be no icon at all.
#[test]
fn a_quad_before_the_drawing_is_under_it_and_a_quad_after_it_is_over_it() {
    let runs = runs!();
    let frame = page::Runs::settled(&runs.layered);

    let above = |_x: i32, y: i32| y < LID_TOP;
    let below = |x: i32, y: i32| (LID_TOP..LID_TOP + 48).contains(&y) && (16..112).contains(&x);

    let ring_above = measure::count(&frame.full, |x, y| in_band(x, y) && above(x, y));
    assert!(
        ring_above > 200,
        "only {ring_above} pixels of the ring are above the lid, so nothing was compared"
    );
    if let Some((x, y, found)) =
        measure::first_unlike(&frame.full, |x, y| in_band(x, y) && above(x, y), INK)
    {
        panic!("({x}, {y}) is ring above the lid and came back {found:?} rather than {INK:?}");
    }

    let under = measure::count(&frame.full, |x, y| in_band(x, y) && below(x, y));
    assert!(
        under > 200,
        "only {under} pixels of the ring are under the lid"
    );
    if let Some((x, y, found)) =
        measure::first_unlike(&frame.full, |x, y| in_band(x, y) && below(x, y), LID)
    {
        panic!(
            "({x}, {y}) is ring under an opaque box drawn after it and came back {found:?}: the \
             composite landed after the box instead of before it"
        );
    }

    // And the panel, drawn before the drawing, is visible through the counter above the lid.
    let hole = measure::count(&frame.full, |x, y| in_counter(x, y) && above(x, y));
    assert!(hole > 200, "only {hole} counter pixels are above the lid");
    if let Some((x, y, found)) =
        measure::first_unlike(&frame.full, |x, y| in_counter(x, y) && above(x, y), PANEL)
    {
        panic!("({x}, {y}) is counter above the lid and came back {found:?} rather than the panel");
    }
}

/// A frame whose document draws nothing plans no rasterisation pass.
#[test]
fn a_document_with_no_drawing_in_it_costs_no_rasterisation() {
    let runs = runs!();
    for frame in &runs.bare {
        assert_eq!(frame.items, 0, "the bare document produced a drawing");
        assert_eq!(
            frame.vector_passes, 0,
            "a frame with no drawing in it planned a rasterisation pass"
        );
    }
    // The other document, on the same device through the same renderer, does plan one — so the
    // count above is about this document rather than about a number nothing ever moves.
    assert!(page::Runs::settled(&runs.marked).vector_passes > 0);
    let bare = page::Runs::settled(&runs.bare);
    if let Some((x, y, found)) = measure::first_unlike(&bare.full, |_, _| true, PANEL) {
        panic!("({x}, {y}) of the bare panel came back {found:?} rather than {PANEL:?}");
    }
}

/// A repaint scissored to the drawing redraws that rectangle and leaves the rest alone.
///
/// The target is filled with a colour that appears in no frame first. Whatever the scissored draw
/// does not touch is still that colour afterwards, and whatever it does touch is the frame.
#[test]
fn a_repaint_scissored_to_the_drawing_touches_that_rectangle_and_nothing_else() {
    let runs = runs!();
    let frame = page::Runs::settled(&runs.marked);
    let rectangle = frame.declared.expect("a drawing was planned");
    let scissored = frame.scissored.as_ref().expect("a scissored repaint ran");

    let inside = |x: i32, y: i32| rectangle.contains(zgui::geom::Point::new(x, y));
    let outside = move |x: i32, y: i32| !inside(x, y);

    // Outside: still the marker, so the draw physically did not write there.
    let untouched = measure::count(scissored, outside);
    assert!(
        untouched > 10_000,
        "only {untouched} pixels lie outside the drawing's rectangle in a {}px window",
        page::SURFACE
    );
    if let Some((x, y, found)) = measure::first_unlike(scissored, outside, MARKER) {
        panic!(
            "({x}, {y}) is outside the damage rectangle {rectangle:?} and came back {found:?} \
             rather than the {MARKER:?} that was on the target before the repaint: the scissor \
             did not hold"
        );
    }

    // Inside: the frame, exactly as a full repaint draws it.
    if let Some((x, y, was, now)) = measure::first_difference(&frame.full, scissored, inside) {
        panic!(
            "({x}, {y}) is inside the damage rectangle and a full repaint drew {was:?} where the \
             scissored one drew {now:?}: the rectangle was not redrawn"
        );
    }
    // And the rectangle really did change, so the comparison above is not two copies of the marker.
    let changed = measure::count(scissored, |x, y| {
        inside(x, y) && scissored.rgba(x, y) != MARKER
    });
    assert!(
        changed > 2_000,
        "only {changed} pixels inside the rectangle differ from what was on the target before the \
         repaint, so nothing was redrawn"
    );
}

/// The same frame reached by a scissored repaint is the same bytes as a full one.
#[test]
fn a_scissored_repaint_is_pixel_identical_to_a_full_one() {
    let runs = runs!();
    let frame = page::Runs::settled(&runs.marked);
    let replayed = frame.replayed.as_ref().expect("a replayed repaint ran");
    if let Some((x, y, full, partial)) =
        measure::first_difference(&frame.full, replayed, |_, _| true)
    {
        panic!(
            "({x}, {y}) came back {full:?} from a full repaint and {partial:?} from a repaint \
             scissored to the drawing"
        );
    }
    // Non-vacuity: the two readbacks are of a picture with an icon in it, not of two blank targets.
    assert!(
        matches!(
            measure::level(replayed, page::centre().0 as i32, 32, INK, PANEL),
            Level::Near | Level::Between
        ),
        "the replayed rendering has no ink at the top of the ring, so the comparison compared two \
         empty panels"
    );
}
