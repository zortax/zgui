//! What a modal surface's scrim covers, on a page that keeps a scrollbar.
//!
//! The rule under test is one sentence: **the dimmed region is the window**. It is not the viewport,
//! and on a page with something to scroll those are two different rectangles — the viewport is the
//! window less the fifteen-pixel strip each scrolling axis reserved for its bar. A scrim is a fixed
//! box, and a fixed box's percentages are of the viewport, so the obvious spelling of "cover
//! everything" leaves those strips lit and a person sees a backdrop with a gap down the right of the
//! screen and along the bottom.
//!
//! # The two readings, and why both
//!
//! The **box** says whether the scrim was asked to cover the window: a rectangle equal to the
//! surface, taken from the engine's own answer for where the node ended up. The **pixels** say
//! whether it did: a scrim mounted at the right size and painted underneath the scrollbar dims
//! nothing a person can see in that strip, and a display list cannot tell that apart from a scrim
//! painted over it. So the gutter is photographed with the dialog shut and again with it open, and
//! the bar has to have gone dark.
//!
//! # How the scrim is picked out of the document
//!
//! By being the largest box with nothing written in it. The overlay bands are boxes with nothing in
//! them too, and they are the window as well — the user-agent sheet sizes the overlay root in
//! viewport units for exactly the reason the scrim needs to be — so a run that found one instead
//! still measures the rectangle under test.

mod desktop;
mod device;
mod painted;

use core::time::Duration;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

use crate::desktop::census::Census;

/// What the dialog's trigger says.
const TRIGGER: &str = "Open dialog";

/// What the dialog says once it is up.
const TITLE: &str = "Rename project";

/// How long everything the tokens animate takes, with room to spare.
const SETTLED: Duration = Duration::from_millis(400);

/// A page larger than any window a fixture here opens, so the root reserves a gutter on both axes.
const SCROLLING: &str = ":root { background-color: #ffffff; color: #101010; overflow: auto }
                         .page { width: 2400px; height: 2000px; padding: 24px;
                                 align-items: flex-start }";

/// The same page, laid out right to left, which is the direction that puts the gutter on the left.
const SCROLLING_RTL: &str = ":root { background-color: #ffffff; color: #101010; overflow: auto;
                                     direction: rtl }
                             .page { width: 2400px; height: 2000px; padding: 24px;
                                     align-items: flex-start }";

/// The same page, small enough to fit, so the root reserves nothing.
const FITTING: &str = ":root { background-color: #ffffff; color: #101010; overflow: auto }
                       .page { width: 200px; height: 100px; padding: 24px;
                               align-items: flex-start }";

/// How thick a reserved gutter is, in CSS pixels.
///
/// The number the layout engine reserves for a classic scrollbar. Named here so that the fixture
/// which asserts "the viewport is narrower than the window" says by how much, and fails rather than
/// passing vacuously in a build that reserves nothing at all.
const GUTTER: f32 = 15.0;

/// A page with a dialog on it.
#[component]
fn Page() -> impl IntoView {
    view! {
        ThemeProvider {
            column(class = "page") {
                Dialog {
                    DialogTrigger {{TRIGGER}}
                    DialogContent {
                        DialogTitle {{TITLE}}
                    }
                }
            }
        }
    }
}

/// A page with a sheet on it, for the edge the sheet is pinned to.
#[component]
fn SheetPage() -> impl IntoView {
    view! {
        ThemeProvider {
            column(class = "page") {
                Sheet {
                    SheetTrigger {"Open sheet"}
                    SheetContent(side = SheetSide::Right) {
                        SheetTitle {"Invoice 4471"}
                    }
                }
            }
        }
    }
}

#[test]
fn a_sheet_reaches_the_window_edge_over_the_scrollbar() {
    // The same rule as the scrim's, for the surface itself: a sheet pinned to the right edge is
    // pinned to the *window's* right edge, not to the viewport's. Left one gutter short, the sheet
    // stands beside a lit strip with the page's scrollbar in it, on a page the scrim has dimmed.
    let mut stage = desktop::stage::Stage::open(SCROLLING, || view! { SheetPage() });
    stage.hold(SETTLED);
    let port = viewport(&stage);
    assert!(
        (desktop::stage::WIDTH - port.width.0 - GUTTER).abs() <= 0.5,
        "the page reserves no gutter, so this fixture cannot tell the window from the viewport"
    );

    stage.click_saying("Open sheet");
    // A sheet arrives at the slowest step of the motion ladder — 500ms — so the ordinary hold
    // still has it a few pixels into its slide.
    stage.hold(Duration::from_millis(800));
    let census = stage.census();
    let sheet = census
        .nodes
        .iter()
        .filter(|node| node.text.contains("Invoice 4471"))
        .filter_map(|node| node.rect)
        .filter(|rect| rect.size.width.0 > 100.0 && rect.size.height.0 > 100.0)
        .min_by(|a, b| {
            (a.size.width.0 * a.size.height.0).total_cmp(&(b.size.width.0 * b.size.height.0))
        })
        .expect("the sheet is open, so its panel has a box");
    let right = sheet.origin.x.0 + sheet.size.width.0;
    assert!(
        (right - desktop::stage::WIDTH).abs() <= 0.5,
        "the sheet's right edge is at {right:.1} and the window's is at {:.1}: the strip between \
         them is the scrollbar gutter the sheet was asked to cover",
        desktop::stage::WIDTH
    );
}

/// The largest box in the document with nothing written in it, which is the scrim.
///
/// # Panics
///
/// Panics when every box in the document says something, because a fixture that quietly measured
/// nothing reports the same thing as a scrim that never mounted.
fn scrim(census: &Census) -> Rect<DevicePx, Device> {
    census
        .nodes
        .iter()
        .filter(|node| node.text.is_empty() && node.area() > 0.0)
        .max_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect)
        .expect("the dialog is open, so something is dimming the window behind it")
}

/// How `rect` reads in a failure.
fn wrote(rect: Rect<DevicePx, Device>) -> String {
    format!(
        "{:.1},{:.1} {:.1}x{:.1}",
        rect.origin.x.0, rect.origin.y.0, rect.size.width.0, rect.size.height.0
    )
}

/// Asserts that `rect` is the whole of the window, naming the case if it is not.
fn assert_is_the_window(rect: Rect<DevicePx, Device>, when: &str) {
    let window = Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(
            DevicePx(desktop::stage::WIDTH),
            DevicePx(desktop::stage::HEIGHT),
        ),
    );
    let slack = 0.5;
    assert!(
        rect.origin.x.0.abs() <= slack
            && rect.origin.y.0.abs() <= slack
            && (rect.size.width.0 - window.size.width.0).abs() <= slack
            && (rect.size.height.0 - window.size.height.0).abs() <= slack,
        "the dimmed region is {} and the window is {} ({when})",
        wrote(rect),
        wrote(window)
    );
}

/// The viewport of the page's own scroll region, in device pixels.
fn viewport(stage: &desktop::stage::Stage) -> Size<DevicePx, Device> {
    let handles = stage.handles();
    let root = handles.dom.root(handles.marker);
    handles.host.scroll_position(root).scrollport
}

#[test]
fn the_scrim_is_the_window_and_not_the_viewport() {
    let mut stage = desktop::stage::Stage::open(SCROLLING, || view! { Page() });
    stage.hold(SETTLED);

    // The premise, asserted rather than assumed: this page really does keep a gutter on both axes,
    // so the window and the viewport really are two rectangles here. Without this a scrim that
    // covered only the viewport would pass the assertion below on any page that reserved nothing,
    // which is every page that fits.
    let port = viewport(&stage);
    assert!(
        (desktop::stage::WIDTH - port.width.0 - GUTTER).abs() <= 0.5
            && (desktop::stage::HEIGHT - port.height.0 - GUTTER).abs() <= 0.5,
        "the page reserves no gutter, so this fixture cannot tell the window from the viewport: \
         the window is {:.0}x{:.0} and the viewport {:.1}x{:.1}",
        desktop::stage::WIDTH,
        desktop::stage::HEIGHT,
        port.width.0,
        port.height.0
    );

    stage.click_saying(TRIGGER);
    stage.hold(SETTLED);
    assert!(
        stage.shows(TITLE),
        "the dialog did not open, so there is no scrim to measure"
    );

    assert_is_the_window(scrim(&stage.census()), "a page that keeps a scrollbar");
}

#[test]
fn the_scrim_is_the_window_in_a_right_to_left_document_too() {
    // The direction decides which side the gutter is reserved on, so a scrim anchored to one named
    // edge covers the window in one direction and is fifteen pixels adrift in the other. The lit
    // strip a failure here leaves is down the *left* of the screen.
    let mut stage = desktop::stage::Stage::open(SCROLLING_RTL, || view! { Page() });
    stage.hold(SETTLED);
    let port = viewport(&stage);
    assert!(
        (desktop::stage::WIDTH - port.width.0 - GUTTER).abs() <= 0.5,
        "the page reserves no gutter, so this fixture proves nothing about which side it is on"
    );

    stage.click_saying(TRIGGER);
    stage.hold(SETTLED);
    assert!(
        stage.shows(TITLE),
        "the dialog did not open, so there is no scrim to measure"
    );
    assert_is_the_window(scrim(&stage.census()), "a right-to-left page");
}

#[test]
fn a_page_that_reserves_nothing_is_covered_exactly_and_gains_no_scrollbar() {
    // The other side of the rule. A window whose viewport *is* its window has to be covered
    // exactly: a scrim reaching past it would be a box larger than the document it is over, which
    // is scrollable overflow — and a modal surface that made the page behind it scrollable would put
    // a scrollbar on a window that had none.
    let mut stage = desktop::stage::Stage::open(FITTING, || view! { Page() });
    stage.hold(SETTLED);
    let before = viewport(&stage);
    assert!(
        (desktop::stage::WIDTH - before.width.0).abs() <= 0.5
            && (desktop::stage::HEIGHT - before.height.0).abs() <= 0.5,
        "this page fits, so it should reserve nothing: the viewport is {:.1}x{:.1}",
        before.width.0,
        before.height.0
    );

    stage.click_saying(TRIGGER);
    stage.hold(SETTLED);
    assert_is_the_window(scrim(&stage.census()), "a page with nothing to scroll");

    let after = viewport(&stage);
    assert!(
        (after.width.0 - before.width.0).abs() <= 0.5
            && (after.height.0 - before.height.0).abs() <= 0.5,
        "opening a dialog reserved a gutter the page did not need: the viewport was {:.1}x{:.1} \
         and is now {:.1}x{:.1}",
        before.width.0,
        before.height.0,
        after.width.0,
        after.height.0
    );
}

#[test]
fn the_scrim_is_still_the_window_at_a_fractional_scale() {
    // The scrim's size is stated in viewport units, which resolve in CSS pixels and are scaled from
    // there — so this is where a rule stated in the wrong space shows up, half again too large or
    // two thirds too small. The surface keeps its device pixels across the change of density, so the
    // window it has to cover is the same rectangle as above.
    let mut stage = desktop::stage::Stage::open(SCROLLING, || view! { Page() });
    stage.present_at(1.5);
    stage.hold(SETTLED);
    stage.click_saying(TRIGGER);
    stage.hold(SETTLED);
    assert_is_the_window(scrim(&stage.census()), "at 1.5 device pixels per CSS pixel");
}

/// How much darker each channel of the gutter has to get before the bar counts as dimmed.
///
/// The scrim is black at forty-five per cent, so a light track under it loses about a hundred
/// levels. Forty is well clear of the dithering of a translucent fill and nowhere near what a scrim
/// missing the strip altogether would produce, which is nothing at all.
const DIMMED: i32 = 40;

/// The middle of the strip the vertical bar occupies.
fn down_the_right() -> Point<DevicePx, Device> {
    Point::new(
        DevicePx(painted::stage::WIDTH - 8.0),
        DevicePx(painted::stage::HEIGHT / 2.0),
    )
}

/// The middle of the strip the horizontal one occupies.
fn along_the_bottom() -> Point<DevicePx, Device> {
    Point::new(
        DevicePx(painted::stage::WIDTH / 2.0),
        DevicePx(painted::stage::HEIGHT - 8.0),
    )
}

#[test]
fn a_scrollbar_under_the_scrim_is_dimmed_rather_than_lit() {
    let Some(mut stage) = painted::stage::Stage::open(SCROLLING, || view! { Page() }) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    stage.wait(SETTLED);
    stage.capture("scrim-shut");
    let lit = (
        stage.colour_at(down_the_right()),
        stage.colour_at(along_the_bottom()),
    );

    let at = painted::words::aim(&stage, TRIGGER);
    stage.click(at);
    stage.wait(SETTLED);
    stage.repaint();
    stage.capture("scrim-open");
    let dimmed = (
        stage.colour_at(down_the_right()),
        stage.colour_at(along_the_bottom()),
    );

    for (edge, before, after) in [
        ("down the right", lit.0, dimmed.0),
        ("along the bottom", lit.1, dimmed.1),
    ] {
        let fell = |before: u8, after: u8| i32::from(before) - i32::from(after);
        assert!(
            fell(before.0, after.0) >= DIMMED
                && fell(before.1, after.1) >= DIMMED
                && fell(before.2, after.2) >= DIMMED,
            "the scrollbar {edge} is lit under the scrim: the strip was {before:?} with the dialog \
             shut and is {after:?} with it open"
        );
    }
}
