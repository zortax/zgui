//! Whether an element that was on the screen is still on it after the frames that follow.
//!
//! # Why a drawing needs fixtures of its own here
//!
//! Everything else a fragment paints is a range of the scene's operation log, and a fragment that
//! has not changed is redrawn by copying that range forward. A drawing is not: its curves become a
//! vector item, planned into a rasterisation pass out of a list the scene rebuilds from nothing
//! every frame. So a drawing is the one kind of content for which *replaying the previous frame*
//! and *drawing the previous frame again* are not the same thing.
//!
//! The failure that shape produces is silent and lasting. The frame that replays the drawing has
//! already cleared the pixels it covers, because the damage is what brought the walk there; nothing
//! is drawn into them; and the frames after that damage the hole no longer, so nothing repaints it.
//! An icon disappears mid-session and stays gone until something forces the whole window to be
//! redrawn — which, on a real desktop, is opening a dialog.
//!
//! So every fixture below drives the window through more frames than one and asks after each of
//! them whether the drawings are still there. The display list is asked as well as the pixels: an
//! icon missing from the display list and an icon whose curves happen to land on page-coloured
//! pixels are the same photograph. An icon reaches the display list as a vector item, a mark item,
//! a coverage sprite or quads, depending on the route its shapes take, so the list is asked for
//! shapes of any of these kinds.

mod desktop;
mod device;
mod painted;

use zgui::geom::{Device, DevicePx, Point, Rect};
use zgui::reactive::RwSignal;
use zgui::view;
use zgui::view::{AnyView, NodeId, NodeRef};
use zgui_ui::prelude::*;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_RIGHT;
use zgui_ui_icons::set::mark::CHECK;
use zgui_ui_tokens::prelude::*;

use crate::device::frame::Drawing;
use crate::painted::stage::Stage;
use crate::painted::words::ink;

/// The page every fixture is laid out on.
///
/// The panels change colour under the pointer. That is what makes a hover damage the rectangle a
/// drawing sits in without changing anything about the drawing itself — which is the whole of what
/// it takes to reach the replay path for it.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 24px; gap: 24px; align-items: flex-start }
                     .panel { padding: 12px; gap: 12px; align-items: center;
                              background-color: #f0f0f0 }
                     .panel:hover { background-color: #e0e0e0 }
                     .port { width: 220px; height: 90px; overflow-y: scroll }
                     .tall { flex-direction: column; gap: 12px; align-items: flex-start }";

/// How much of a drawing's own rectangle has to be marked before it counts as on the screen.
///
/// Small, because these are line icons: a chevron in a twenty-pixel box marks a few per cent of it
/// and no more, so a threshold set by eye from a filled shape would fail every one of them.
const MARKED: f32 = 0.02;

/// Opens `view`, or reports the run skipped on a machine with no graphics device.
macro_rules! staged {
    ($view:expr) => {
        match Stage::open(SHEET, $view) {
            Some(stage) => stage,
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    };
}

/// Where a fixture leaves the elements it built, so a test can find them again.
#[derive(Clone, Default)]
struct Built(std::rc::Rc<std::cell::RefCell<Vec<NodeRef>>>);

impl Built {
    /// Remembers the references a fixture bound, in the order it bound them.
    fn keep(&self, refs: &[NodeRef]) {
        *self.0.borrow_mut() = refs.to_vec();
    }

    /// The element behind the `index`th reference.
    ///
    /// # Panics
    ///
    /// Panics when the fixture never bound it, because a test that went on would be measuring a
    /// node that does not exist.
    fn node(&self, index: usize) -> NodeId {
        self.0
            .borrow()
            .get(index)
            .and_then(NodeRef::get)
            .unwrap_or_else(|| panic!("the fixture bound no reference {index}"))
    }
}

/// Two panels of drawings side by side, and somewhere beside them for the pointer to go.
fn panels(built: &Built) -> impl Fn() -> AnyView + use<> {
    let built = built.clone();
    move || {
        let page = NodeRef::new();
        let left = NodeRef::new();
        let right = NodeRef::new();
        built.keep(&[page, left, right]);
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page", node_ref = page) {
                    row(class = "panel", node_ref = left) {
                        Icon(icon = CHEVRON_RIGHT, size = IconSize::Md)
                    }
                    row(class = "panel", node_ref = right) {
                        Icon(icon = CHECK, size = IconSize::Md)
                    }
                    text {"elsewhere"}
                }
            }
        })
    }
}

/// How many rows of drawings the scrollport holds.
const ROWS: usize = 8;

/// A scrollport with a column of drawings taller than it is.
///
/// The port is the first reference and each row is one after it.
fn scrolled(built: &Built) -> impl Fn() -> AnyView + use<> {
    let built = built.clone();
    move || {
        let port = NodeRef::new();
        let refs: Vec<NodeRef> = (0..ROWS).map(|_| NodeRef::new()).collect();
        let mut kept = vec![port];
        kept.extend(refs.iter().copied());
        built.keep(&kept);
        let rows = refs.into_iter().map(|row| {
            view! {
                row(class = "panel", node_ref = row) {Icon(icon = CHECK, size = IconSize::Md)}
            }
        });
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    column(class = "port", node_ref = port) {
                        column(class = "tall") {{rows.collect::<Vec<_>>()}}
                    }
                }
            }
        })
    }
}

/// A select, whose trigger carries a chevron, and a checkbox that opens nothing.
fn select(built: &Built) -> impl Fn() -> AnyView + use<> {
    let built = built.clone();
    move || {
        let trigger = NodeRef::new();
        built.keep(&[trigger]);
        let chosen = RwSignal::new_local(String::from("euro"));
        AnyView::new(view! {
            ThemeProvider {
                column(class = "page") {
                    Select(value = chosen) {
                        box(node_ref = trigger) {
                            SelectTrigger {SelectValue()}
                        }
                        SelectContent {
                            SelectItem(value = "euro") {"Euro"}
                            SelectItem(value = "pound") {"Pound sterling"}
                        }
                    }
                    text {"elsewhere"}
                }
            }
        })
    }
}

/// What reads the shapes of the drawings inside a rectangle off the last frame that drew.
type Shapes = fn(&Stage, Rect<DevicePx, Device>) -> Vec<Drawing>;

/// Asserts that every drawing inside `rect` reached the display list of the last frame that drew.
///
/// The display list rather than the pixels, because that is the question an ordinary frame can
/// answer. A window repaints the rectangles it damaged and nothing else, so the frame after an
/// interaction holds the control that changed and not the page around it — but if the damage
/// brought the walk to a drawing at all, the frame that cleared those pixels has to have drawn it.
///
/// Each icon here is one shape, so `expected` counts icons.
fn assert_listed(
    stage: &Stage,
    shapes: Shapes,
    rect: Rect<DevicePx, Device>,
    expected: usize,
    when: &str,
) {
    let drawings = shapes(stage, rect);
    assert_eq!(
        drawings.len(),
        expected,
        "{when}: the frame that repainted {rect:?} held {} of the {expected} drawings in it",
        drawings.len()
    );
}

/// Asserts that every drawing inside `rect` is in the display list *and* has marked its own pixels.
///
/// Only meaningful over a whole-window frame. The second half is what a display list cannot answer:
/// a drawing composited into the wrong target, or under a clip that excludes it, is listed and
/// invisible.
fn assert_drawn(
    stage: &Stage,
    shapes: Shapes,
    rect: Rect<DevicePx, Device>,
    expected: usize,
    when: &str,
) {
    assert_listed(stage, shapes, rect, expected, when);
    for drawing in shapes(stage, rect) {
        let marked = ink(stage, padded(drawing.ink));
        assert!(
            marked > MARKED,
            "{when}: a drawing at {:?} left its own rectangle flat ({marked} marked)",
            drawing.ink
        );
    }
}

/// `rect` with a margin of page around it.
///
/// The ink reading counts pixels differing from the commonest colour *inside the rectangle it is
/// given*, so a rectangle fitted tightly to a stroke is mostly stroke and reads as flat — which is
/// the same answer it gives for a drawing that was never drawn. A margin puts enough of the page in
/// the reading for the page to be the colour the outline is measured against.
fn padded(rect: Rect<DevicePx, Device>) -> Rect<DevicePx, Device> {
    const MARGIN: f32 = 6.0;
    Rect::new(
        Point::new(
            DevicePx(rect.origin.x.0 - MARGIN),
            DevicePx(rect.origin.y.0 - MARGIN),
        ),
        zgui::geom::Size::new(
            DevicePx(rect.size.width.0 + MARGIN * 2.0),
            DevicePx(rect.size.height.0 + MARGIN * 2.0),
        ),
    )
}

/// The defect at the smallest scale that produces it: repaint a panel twice with a drawing in it.
///
/// The pointer arrives on the panel, the panel takes its hover colour and the damage covers the
/// icon; the pointer leaves, the panel gives the colour back and the damage covers the icon again.
/// The icon's own style, box, clip and transform are the same on both frames — which is exactly the
/// condition a replay is for, and exactly the condition under which a replayed drawing draws
/// nothing while its pixels have already been cleared.
#[test]
fn a_drawing_is_in_every_ordinary_frame_that_repaints_the_panel_it_sits_in() {
    let built = Built::default();
    let mut stage = staged!(panels(&built));
    let panel = stage.rect_of(built.node(1));
    let elsewhere = Point::new(DevicePx(8.0), DevicePx(8.0));

    for round in 1..=5 {
        stage.move_to(stage.centre_of(built.node(1)));
        stage.settle();
        assert_listed(
            &stage,
            Stage::shapes_in,
            panel,
            1,
            &format!("hovered, round {round}"),
        );
        stage.move_to(elsewhere);
        stage.settle();
        assert_listed(
            &stage,
            Stage::shapes_in,
            panel,
            1,
            &format!("unhovered, round {round}"),
        );
    }
    stage.repaint();
    for side in [1, 2] {
        let panel = stage.rect_of(built.node(side));
        assert_drawn(&stage, Stage::shapes_in, panel, 1, "after five sweeps");
    }
    stage.capture("redrawn-hover");
}

/// The rows of [`scrolled`] whose whole box is inside the scrollport now.
///
/// Rows rather than the port, because the port also holds its scrollbar, and a thumb is a shape
/// that is drawn whether or not any icon is.
fn shown_rows(stage: &Stage, built: &Built) -> Vec<Rect<DevicePx, Device>> {
    let port = stage.rect_of(built.node(0));
    (1..=ROWS)
        .map(|index| stage.rect_of(built.node(index)))
        .filter(|row| {
            row.origin.y.0 >= port.origin.y.0
                && row.origin.y.0 + row.size.height.0 <= port.origin.y.0 + port.size.height.0
        })
        .collect()
}

/// How many shapes the last frame that drew held inside the rows shown in the scrollport.
fn shown_icons(stage: &Stage, built: &Built) -> usize {
    shown_rows(stage, built)
        .into_iter()
        .map(|row| stage.shapes_in(row).len())
        .sum()
}

/// A drawing carried through a scroll is still drawn on the frames that carry it.
///
/// Scrolling is the case that replays most: the rows are the same rows in the same styles at new
/// places, which is exactly what a translated replay is for. A scrollport is where the largest
/// number of drawings would go out at once.
#[test]
fn drawings_in_a_scrollport_are_still_drawn_while_it_is_scrolled() {
    let built = Built::default();
    let mut stage = staged!(scrolled(&built));
    assert!(
        shown_icons(&stage, &built) > 0,
        "the scrollport drew no icons to begin with"
    );

    stage.move_to(stage.centre_of(built.node(0)));
    for round in 1..=4 {
        stage.wheel(-1.0);
        stage.settle();
        assert!(
            shown_icons(&stage, &built) > 0,
            "scroll round {round}: the frame that repainted the scrollport held no drawings"
        );
    }
    stage.repaint();
    let rows = shown_rows(&stage, &built);
    assert!(!rows.is_empty(), "no row is whole inside the scrollport");
    for row in rows {
        assert_drawn(&stage, Stage::shapes_in, row, 1, "after four detents");
    }
}

/// The shapes the last frame that drew held inside the trigger, without its text.
///
/// The text is removed by the box of the smallest node that says it, because a glyph can arrive
/// as a coverage sprite exactly as the chevron can.
fn chevrons(stage: &Stage, trigger: Rect<DevicePx, Device>) -> Vec<Drawing> {
    let census = stage.census();
    let text = census
        .nodes
        .iter()
        .filter(|node| node.text == "Euro")
        .filter_map(|node| node.rect)
        .filter(|rect| middle_of(*rect, trigger))
        .min_by(|left, right| {
            (left.size.width.0 * left.size.height.0)
                .total_cmp(&(right.size.width.0 * right.size.height.0))
        });
    stage
        .shapes_in(trigger)
        .into_iter()
        .filter(|shape| text.is_none_or(|rect| !middle_of(shape.ink, rect)))
        .collect()
}

/// Whether the middle of `inner` is inside `outer`.
fn middle_of(inner: Rect<DevicePx, Device>, outer: Rect<DevicePx, Device>) -> bool {
    let x = inner.origin.x.0 + inner.size.width.0 / 2.0;
    let y = inner.origin.y.0 + inner.size.height.0 / 2.0;
    x >= outer.origin.x.0
        && x <= outer.origin.x.0 + outer.size.width.0
        && y >= outer.origin.y.0
        && y <= outer.origin.y.0 + outer.size.height.0
}

/// The control the defect was seen on: a select trigger whose chevron went out and stayed out.
///
/// The list is opened over the page and closed again, and after every one of those the chevron has
/// to still be there. Opening a surface over the page is what used to hide the fault — it forces
/// the whole window to be redrawn, which puts the drawing back — so the assertion is made after the
/// close, once the frames that follow are ordinary ones again.
#[test]
fn a_select_triggers_chevron_is_still_drawn_after_the_list_has_been_opened_and_closed() {
    let built = Built::default();
    let mut stage = staged!(select(&built));
    let trigger = stage.rect_of(built.node(0));
    stage.repaint();
    assert_drawn(&stage, chevrons, trigger, 1, "the first frame");

    // The rounds are collected and reported together rather than asserted inside the loop. A
    // failure raised while the list is still animating out unwinds a document with a surface half
    // torn down, and what reaches the log is that teardown rather than the round that failed.
    let mut lost = Vec::new();
    for round in 1..=4 {
        stage.click(stage.centre_of(built.node(0)));
        stage.settle();
        stage.press_named(zgui::vocab::NamedKey::Escape);
        stage.settle();
        // Only a frame that came to the trigger can say anything about it. The last frame of a
        // close repaints the region the list and its shadow left behind, and whether the trigger
        // is inside that region is a fact about how far a popover's shadow reaches — nothing to do
        // with drawings. A frame that never reached the trigger holds none of its glyphs either,
        // and that is what tells the two apart: a repainted trigger brings its own text with it.
        if stage.glyphs_in(trigger).is_empty() {
            continue;
        }
        if chevrons(&stage, trigger).len() != 1 {
            lost.push(round);
        }
    }
    stage.repaint();
    assert!(
        lost.is_empty(),
        "the trigger's chevron was missing from the frame that repainted it after cycles {lost:?}"
    );
    assert_drawn(&stage, chevrons, trigger, 1, "after four cycles");
    stage.capture("redrawn-select-chevron");
}
