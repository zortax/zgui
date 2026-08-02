//! Where a floating list actually lands, in the numbers a person would recognise.
//!
//! Every surface in this library is placed by one component, and it is placed from measurements the
//! frame delivers: the trigger's box, the surface's own size, the window's rectangle. All three of
//! those arrive in **device** pixels, and the answer is written back as an inline `left` and `top`
//! — which a style sheet reads as **CSS** pixels. On the display nearly every fixture runs on those
//! two are the same number, so a confusion between them costs nothing and is invisible; on a denser
//! one it multiplies the surface's position by the density and opens a select's list below and to
//! the right of the whole card its trigger is on.
//!
//! So the fixtures here are asked at 1.25 device pixels per CSS pixel, and the question they ask is
//! the one somebody looking at the window asks: is the list's top edge against the bottom of the
//! control that opened it, and does its left edge line up with that control's?

mod desktop;

use core::time::Duration;

use zgui::geom::{Device, DevicePx, Point, Rect};
use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::desktop::census::Seen;
use crate::desktop::stage::{Stage, WIDTH};

/// The page the fixtures are laid out on.
///
/// `.probe` is the one rule doing work: it shrink-wraps the box around a trigger, so that box *is*
/// the trigger's box and a fixture can find it by what the trigger says. `flex-shrink: 0` on the
/// spacers is the second: a flex item is shrunk to fit by default, so a spacer without it makes a
/// page that fits the window exactly and never scrolls.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif;
                             overflow: auto }
                     .page { padding: 40px; gap: 40px; align-items: flex-start }
                     .card { padding: 32px; border: 1px solid #d0d0d0; gap: 24px }
                     .probe { align-self: flex-start }
                     .probe.at-right { align-self: flex-end }
                     .tall { height: 1200px; flex-shrink: 0 }
                     .region { height: 220px; width: 340px; overflow: auto;
                               border: 1px solid #d0d0d0 }
                     .inside { height: 900px; flex-shrink: 0; padding: 24px; gap: 24px }";

/// How far a surface is asked to sit off its trigger, in CSS pixels: the library's own default.
const GAP: f32 = 4.0;

/// The density the defect lives at, and the one no other fixture in this package runs at.
///
/// Every fixture below is asked at this one, because at one device pixel per CSS pixel the two
/// spaces a placement passes through coincide and a confusion between them changes nothing. The
/// plain density is asked once, on its own, so that a failure says which of the two it is.
const DENSE: f64 = 1.25;

/// What the trigger of every fixture's select says while nothing is chosen.
const TRIGGER: &str = "Choose one";

/// What its list says, all of it, which is how the list is found.
const LIST: &str = "Pound sterlingEuro";

/// How far a rectangle may be from where it belongs before the window looks wrong.
const SLACK: f32 = 2.0;

/// A select whose trigger is shrink-wrapped, so the box saying [`TRIGGER`] is the trigger's box.
#[component]
fn Probe(
    /// Classes merged after the probe's own.
    #[prop(into, optional)]
    class: Classes,
) -> impl IntoView {
    let currency = RwSignal::new_local(String::new());
    view! {
        row(class = "probe", class = class) {
            Select(value = currency) {
                SelectTrigger(a11y:label = "Currency") {
                    SelectValue(placeholder = TRIGGER)
                }
                SelectContent {
                    SelectItem(value = "gbp") {"Pound sterling"}
                    SelectItem(value = "eur") {"Euro"}
                }
            }
        }
    }
}

/// The box of the outermost *laid-out* thing saying `text`.
///
/// Laid out, because the same words are on nodes that have no box at all — a closed select keeps a
/// catalogue of what its options read as, out of sight — and the outermost of those would answer
/// `None` and take every assertion below it with it. Every fixture here puts something else inside
/// each of the probe's ancestors, so the outermost box saying what the trigger says is the probe's,
/// which shrink-wraps the trigger and is therefore the trigger's own rectangle.
fn box_saying(stage: &Stage, text: &str) -> Rect<DevicePx, Device> {
    laid_out(stage, text)
        .into_iter()
        .min_by_key(|seen| seen.depth)
        .and_then(|seen| seen.rect)
        .unwrap_or_else(|| panic!("nothing laid out says {text:?}"))
}

/// The box of the innermost laid-out thing saying `text`, which for an open list is its panel.
///
/// The innermost rather than the outermost, because a portalled surface hangs off an overlay band
/// that holds nothing but the surface — so the band says exactly what the list says and is the size
/// of the whole window.
fn innermost_box_saying(stage: &Stage, text: &str) -> Rect<DevicePx, Device> {
    laid_out(stage, text)
        .into_iter()
        .max_by_key(|seen| seen.depth)
        .and_then(|seen| seen.rect)
        .unwrap_or_else(|| panic!("nothing laid out says {text:?}"))
}

/// Every node whose whole text is `text` and which has a box with room in it.
fn laid_out(stage: &Stage, text: &str) -> Vec<Seen> {
    stage
        .census()
        .nodes
        .into_iter()
        .filter(|seen| seen.text == text && seen.area() > 0.0)
        .collect()
}

/// Asserts that the list is directly under the trigger, at `density` device pixels per CSS pixel.
///
/// Two numbers, both of which a person reads off the window: how far the list's top edge is below
/// the trigger's bottom edge, and how far its left edge is from the trigger's. The gap is stated in
/// CSS pixels by the component and read in device pixels here, so it is converted rather than
/// widened into a range that would accept a placement multiplied by the density.
fn assert_under(trigger: Rect<DevicePx, Device>, list: Rect<DevicePx, Device>, density: f64) {
    let wanted = GAP * density as f32;
    let gap = list.origin.y.0 - (trigger.origin.y.0 + trigger.size.height.0);
    let across = list.origin.x.0 - trigger.origin.x.0;
    assert!(
        (gap - wanted).abs() <= SLACK,
        "the list's top edge is {gap} device pixels below the trigger's bottom edge rather than \
         {wanted}; trigger {trigger:?}, list {list:?}"
    );
    assert!(
        across.abs() <= SLACK,
        "the list's left edge is {across} device pixels from the trigger's; trigger {trigger:?}, \
         list {list:?}"
    );
}

/// Opens `view` on a window presented at `density`.
fn opened<F, V>(density: f64, view: F) -> Stage
where
    F: FnMut() -> V + 'static,
    V: IntoView,
{
    let mut stage = Stage::open(SHEET, view);
    if (density - 1.0).abs() > f64::EPSILON {
        stage.present_at(density);
    }
    stage
}

/// Turns the wheel over `at` until the trigger's box satisfies `wanted`.
///
/// # Panics
///
/// Panics when it never does, because a fixture that gave up quietly would go on to assert about a
/// page that never moved and pass without asking anything.
fn scroll_until(
    stage: &mut Stage,
    at: Point<DevicePx, Device>,
    wanted: impl Fn(Rect<DevicePx, Device>) -> bool,
) -> Rect<DevicePx, Device> {
    stage.move_to(at);
    for _ in 0..120 {
        let seen = laid_out(stage, TRIGGER)
            .into_iter()
            .min_by_key(|seen| seen.depth)
            .and_then(|seen| seen.rect);
        match seen {
            Some(rect) if wanted(rect) => return rect,
            _ => stage.wheel(1.0),
        }
    }
    panic!("the trigger never came to where this fixture needs it, so nothing was asked");
}

// ---- the shape every application is ------------------------------------------------------------

/// A trigger several boxes into a page, on a window of `density`.
///
/// The one scenario asked at both densities, so that a failure separates *the placement is wrong*
/// from *the placement is wrong on a dense display*.
fn a_select_deep_in_a_page(density: f64) {
    let mut stage = opened(density, || {
        view! {
            ThemeProvider {
                column(class = "page") {
                    column(class = "card") {text {"Payment"}Probe()}
                }
            }
        }
    });
    let trigger = box_saying(&stage, TRIGGER);
    assert!(
        trigger.origin.x.0 > 60.0 && trigger.origin.y.0 > 60.0,
        "the trigger is at {trigger:?}, near enough the corner that a placement multiplied by \
         {density} would still land on it"
    );

    stage.click_saying(TRIGGER);
    stage.settle();

    assert_under(trigger, innermost_box_saying(&stage, LIST), density);
}

#[test]
fn a_select_deep_in_a_page_opens_its_list_under_its_trigger() {
    a_select_deep_in_a_page(1.0);
}

#[test]
fn a_select_deep_in_a_page_opens_its_list_under_its_trigger_on_a_dense_display() {
    a_select_deep_in_a_page(DENSE);
}

#[test]
fn a_select_at_the_top_of_a_page_opens_its_list_under_its_trigger() {
    let mut stage = opened(DENSE, || {
        view! {
            ThemeProvider {
                column(class = "page") {Probe()text {"under it"}}
            }
        }
    });
    let trigger = box_saying(&stage, TRIGGER);

    stage.click_saying(TRIGGER);
    stage.settle();

    assert_under(trigger, innermost_box_saying(&stage, LIST), DENSE);
}

// ---- the window's edges still have their say ---------------------------------------------------

#[test]
fn a_select_at_the_bottom_of_the_window_opens_its_list_above_its_trigger() {
    let mut stage = opened(DENSE, || {
        view! {
            ThemeProvider {
                column(class = "page") {
                    text {"over it"}
                    box(class = "tall")
                    Probe()
                }
            }
        }
    });
    // Into the window and against its bottom edge, which is the only place the flip is forced.
    let trigger = scroll_until(
        &mut stage,
        Point::new(DevicePx(600.0), DevicePx(400.0)),
        |rect| (620.0..870.0).contains(&(rect.origin.y.0 + rect.size.height.0)),
    );

    stage.click_saying(TRIGGER);
    stage.settle();

    let list = innermost_box_saying(&stage, LIST);
    let wanted = GAP * DENSE as f32;
    let gap = trigger.origin.y.0 - (list.origin.y.0 + list.size.height.0);
    assert!(
        (gap - wanted).abs() <= SLACK,
        "there was no room below, so the list's bottom edge belongs {wanted} device pixels \
         above the trigger's top edge rather than {gap}; trigger {trigger:?}, list {list:?}"
    );
    assert!(
        (list.origin.x.0 - trigger.origin.x.0).abs() <= SLACK,
        "a flipped list is still aligned with its trigger; trigger {trigger:?}, list {list:?}"
    );
}

#[test]
fn a_select_against_the_right_edge_shifts_its_list_back_inside_the_window() {
    let mut stage = opened(DENSE, || {
        view! {
            ThemeProvider {
                column(class = "page") {Probe(class = "at-right")text {"under it"}}
            }
        }
    });
    let trigger = box_saying(&stage, TRIGGER);
    assert!(
        trigger.origin.x.0 + trigger.size.width.0 > WIDTH - 80.0,
        "the trigger is not against the right edge, so nothing has to shift: {trigger:?}"
    );

    stage.click_saying(TRIGGER);
    stage.settle();

    let list = innermost_box_saying(&stage, LIST);
    assert!(
        list.origin.x.0 + list.size.width.0 <= WIDTH,
        "the list runs off the right of the window: {list:?}"
    );
    assert!(
        list.origin.x.0 <= trigger.origin.x.0 + SLACK,
        "a list kept inside the window never moves further out than its trigger: {list:?} \
         against {trigger:?}"
    );
    let wanted = GAP * DENSE as f32;
    let gap = list.origin.y.0 - (trigger.origin.y.0 + trigger.size.height.0);
    assert!(
        (gap - wanted).abs() <= SLACK,
        "shifting the list sideways moved it {gap} device pixels down as well"
    );
}

// ---- the page and the panels move under it -----------------------------------------------------

#[test]
fn a_select_opened_after_the_page_has_scrolled_since_it_last_opened_follows_its_trigger() {
    // The sequence nothing else covers: the surface is placed, closed, and the page moves under it
    // before it is asked for again. A placement made from anything but this frame's measurement
    // opens the second time where the first one was.
    let mut stage = opened(DENSE, || {
        view! {
            ThemeProvider {
                column(class = "page") {
                    text {"over it"}
                    box(class = "tall")
                    Probe()
                    box(class = "tall")
                }
            }
        }
    });
    let aim = Point::new(DevicePx(600.0), DevicePx(400.0));
    let first = scroll_until(&mut stage, aim, |rect| {
        (400.0..600.0).contains(&rect.origin.y.0)
    });
    stage.click_saying(TRIGGER);
    stage.settle();
    assert_under(first, innermost_box_saying(&stage, LIST), DENSE);
    // The clock as well as the frames: a surface is kept mounted through its exit animation, so a
    // list that has been dismissed and not yet faded out is still a box in the window.
    stage.key(zgui::vocab::NamedKey::Escape);
    stage.hold(Duration::from_millis(400));
    assert!(
        laid_out(&stage, LIST).is_empty(),
        "Escape left the list open, so the press below closes it instead of re-opening it"
    );

    let trigger = scroll_until(&mut stage, aim, |rect| {
        rect.origin.y.0 < first.origin.y.0 - 200.0 && rect.origin.y.0 > 40.0
    });

    stage.click_saying(TRIGGER);
    stage.settle();

    assert_under(trigger, innermost_box_saying(&stage, LIST), DENSE);
}

#[test]
fn a_select_inside_a_scrolled_region_opens_its_list_under_its_trigger() {
    let mut stage = opened(DENSE, || {
        view! {
            ThemeProvider {
                column(class = "page") {
                    column(class = "region") {
                        column(class = "inside") {text {"above"}Probe()}
                    }
                }
            }
        }
    });
    let before = box_saying(&stage, TRIGGER);
    let trigger = scroll_until(
        &mut stage,
        Point::new(DevicePx(120.0), DevicePx(120.0)),
        |rect| rect.origin.y.0 < before.origin.y.0 - 40.0,
    );

    stage.click_saying(TRIGGER);
    stage.settle();

    assert_under(trigger, innermost_box_saying(&stage, LIST), DENSE);
}

// ---- inside another surface --------------------------------------------------------------------

#[test]
fn a_select_inside_a_dialog_opens_its_list_under_its_trigger() {
    let mut stage = opened(DENSE, || {
        view! {
            ThemeProvider {
                column(class = "page") {
                    Dialog {
                        DialogTrigger {"Rename…"}
                        DialogContent {
                            DialogTitle {"Rename project"}
                            Probe()
                        }
                    }
                }
            }
        }
    });
    stage.click_saying("Rename…");
    stage.hold(Duration::from_millis(400));
    let trigger = box_saying(&stage, TRIGGER);

    stage.click_saying(TRIGGER);
    stage.hold(Duration::from_millis(400));

    assert_under(trigger, innermost_box_saying(&stage, LIST), DENSE);
}
