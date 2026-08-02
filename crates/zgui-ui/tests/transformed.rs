//! Clicking text that has been turned.
//!
//! A transform is applied when a box is *drawn*, not when it is laid out: the rectangle layout
//! recorded is the one the box would have had upright, and the glyphs reach the screen through a
//! matrix. So there are two places a pointer can be compared with the text and only one of them is
//! right. Comparing it with the recorded rectangle produces a field that reads perfectly, looks
//! turned, and puts the caret in front of whatever character *would* have been under the pointer if
//! it had not been turned — which no assertion about the display list, the layout or the
//! accessibility tree can see, because every one of those is right.
//!
//! The question is asked twice against the same field: once upright and once turned, clicking the
//! same place on the run both times. Whatever the font on this machine measures, the two answers
//! have to be the same answer.

mod desktop;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::{component, view};

use crate::desktop::census::absolute;
use crate::desktop::stage::Stage;

/// How far the turned field is rotated.
const ANGLE: f32 = -30.0;

/// The sheet: two identical fields, one of them turned about its own middle.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
     .page { padding: 60px; gap: 90px; align-items: flex-start }
     .twin { width: 320px; font-size: 20px; }
     .turned { transform: rotate(-30deg); }";

/// What both fields hold, which is long enough that one character is a small part of the run.
const START: &str = "ABCDEFGHIJKLMNOP";

/// Two identical fields, one turned.
#[component]
fn Twins(
    /// Where to record the upright field's element.
    upright_ref: NodeRef,
    /// Where to record the turned one's.
    turned_ref: NodeRef,
) -> impl IntoView {
    let upright = RwSignal::new_local(START.to_owned());
    let turned = RwSignal::new_local(START.to_owned());

    view! {
        column(class = "page") {
            editor(class = "twin", node_ref = upright_ref, tabindex = {Focus::Sequential}) {
                {move || upright.get()}
            }
            editor(class = "twin turned", node_ref = turned_ref, tabindex = {Focus::Sequential}) {
                {move || turned.get()}
            }
        }
    }
}

/// The middle of a rectangle.
fn centre(rect: Rect<DevicePx, Device>) -> Point<DevicePx, Device> {
    Point::new(
        DevicePx(rect.origin.x.0 + rect.size.width.0 / 2.0),
        DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
    )
}

/// `point`, turned by `degrees` about `about`.
///
/// Which is where a rotated box's own transform puts it on the screen: a transform origin defaults
/// to the middle of the box, so this is the same matrix the paint stage applies.
fn turned(
    point: Point<DevicePx, Device>,
    about: Point<DevicePx, Device>,
    degrees: f32,
) -> Point<DevicePx, Device> {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let x = point.x.0 - about.x.0;
    let y = point.y.0 - about.y.0;
    Point::new(
        DevicePx(about.x.0 + x * cos - y * sin),
        DevicePx(about.y.0 + x * sin + y * cos),
    )
}

/// How big the fields are, taken from the one that has not been turned.
///
/// The upright twin is under no transform, so where it is on the screen and how big it is are the
/// same rectangle — and the two are declared identical, so this is the turned one's size as well.
fn twin_size(stage: &Stage, upright: NodeRef) -> Size<DevicePx, Device> {
    let node = upright.get_untracked().expect("the field is in the tree");
    absolute(stage.handles(), node)
        .expect("the field was laid out")
        .size
}

/// A field's own rectangle, upright, about the middle of where it actually landed.
///
/// [`absolute`] answers where a box is *on the screen*, transform and all, which for a turned box
/// is the smallest upright rectangle containing it — wider and shorter than the field. A point on
/// the run has to be chosen in the field's own space instead, and the way back to it is the twin:
/// the two fields are declared identical, so `size` is this one's size too, and a rotation about a
/// box's own middle leaves that middle where it is, so the middle of what was measured is the
/// middle of the field.
fn own_rect(stage: &Stage, field: NodeRef, size: Size<DevicePx, Device>) -> Rect<DevicePx, Device> {
    let node = field.get_untracked().expect("the field is in the tree");
    let placed = absolute(stage.handles(), node).expect("the field was laid out");
    let (middle_x, middle_y) = {
        let middle = centre(placed);
        (middle.x.0, middle.y.0)
    };
    Rect::new(
        Point::new(
            DevicePx(middle_x - size.width.0 / 2.0),
            DevicePx(middle_y - size.height.0 / 2.0),
        ),
        size,
    )
}

/// Clicks `fraction` of the way along a field and reports where its caret went.
///
/// The point is chosen in the field's *own* space and then put where the transform puts it, which
/// is exactly what a person sees and aims at. `size` is the field's own size, which comes from its
/// upright twin — see [`own_rect`].
fn caret_after_click(
    stage: &mut Stage,
    field: NodeRef,
    size: Size<DevicePx, Device>,
    degrees: f32,
    fraction: f32,
) -> usize {
    let rect = own_rect(stage, field, size);
    let local = Point::new(
        DevicePx(rect.origin.x.0 + rect.size.width.0 * fraction),
        DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
    );
    stage.click(turned(local, centre(rect), degrees));
    stage.settle();
    field
        .selection()
        .unwrap_or_else(|| panic!("the field reports no selection after being clicked"))
        .start
}

#[test]
fn a_click_on_a_turned_field_lands_on_the_character_it_is_over() {
    let upright_ref = NodeRef::new();
    let turned_ref = NodeRef::new();
    let mut stage = Stage::open(SHEET, move || {
        view! { Twins(upright_ref = upright_ref, turned_ref = turned_ref) }
    });

    // A third of the way along the run: inside the text, away from either edge, so a rounding
    // disagreement cannot move the answer by a whole character.
    let size = twin_size(&stage, upright_ref);
    let flat = caret_after_click(&mut stage, upright_ref, size, 0.0, 0.34);
    let tilted = caret_after_click(&mut stage, turned_ref, size, ANGLE, 0.34);

    assert!(
        flat > 0 && flat < START.len(),
        "the upright field put its caret at {flat}, which is an end rather than the character \
         under the pointer — so this fixture is measuring nothing"
    );
    assert_eq!(
        tilted, flat,
        "the same place on the same run, once upright and once turned {ANGLE} degrees, put the \
         caret in front of two different characters — so the caret hit test is comparing the \
         pointer with the rectangle the field would have had if it had not been turned"
    );
}

#[test]
fn clicks_along_a_turned_run_walk_the_caret_the_way_the_run_goes() {
    let upright_ref = NodeRef::new();
    let turned_ref = NodeRef::new();
    let mut stage = Stage::open(SHEET, move || {
        view! { Twins(upright_ref = upright_ref, turned_ref = turned_ref) }
    });

    // Four places along the run. Upright they give four rising offsets; turned they have to give
    // the same four. Rising alone would not be enough: a hit test that ignores the transform is
    // still reading a real run, just the wrong place in it, so its answers rise too.
    let places = [0.08_f32, 0.24, 0.40, 0.56];
    let size = twin_size(&stage, upright_ref);
    let flat: Vec<usize> = places
        .iter()
        .map(|at| caret_after_click(&mut stage, upright_ref, size, 0.0, *at))
        .collect();
    let tilted: Vec<usize> = places
        .iter()
        .map(|at| caret_after_click(&mut stage, turned_ref, size, ANGLE, *at))
        .collect();

    assert!(
        flat.windows(2).all(|pair| pair[1] > pair[0]),
        "the upright field does not walk its caret along the run at all: {flat:?}"
    );
    assert_eq!(
        tilted, flat,
        "four places along the run answered differently once the run was turned"
    );
}

#[test]
fn a_click_where_a_turned_field_is_not_puts_no_caret_in_it() {
    let upright_ref = NodeRef::new();
    let turned_ref = NodeRef::new();
    let mut stage = Stage::open(SHEET, move || {
        view! { Twins(upright_ref = upright_ref, turned_ref = turned_ref) }
    });
    let node = turned_ref
        .get_untracked()
        .expect("the field is in the tree");
    let rect = absolute(stage.handles(), node).expect("the field was laid out");

    // The right-hand end of the rectangle layout recorded. The field has been turned away from
    // there, so on the screen that place is blank page.
    let away = Point::new(
        DevicePx(rect.origin.x.0 + rect.size.width.0 - 4.0),
        DevicePx(rect.origin.y.0 + rect.size.height.0 / 2.0),
    );
    stage.click(away);
    stage.settle();

    assert_ne!(
        stage.focused(),
        Some(node),
        "a click on blank page beside the turned field focused it anyway, so the hit test is \
         using the rectangle the field would have had upright"
    );
}
