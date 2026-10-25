//! Whether a list scrolled far down draws its rows on the pixels a list at its top does.
//!
//! A port that scrolled draws its rows in a content space of its own, at coordinates as large as
//! the scroll offset. A port built again at that offset draws them on the device until it scrolls.
//! The two routes, and a list that never scrolled, must all put the same colour on every pixel.
//!
//! The defect these tests catch shows only on a GPU whose f32 interpolation is about 1 ULP off near
//! 22 000 px.

mod desktop;
mod device;
mod painted;

use std::cell::Cell;
use std::rc::Rc;

use zgui::geom::{Device, DevicePx, Point, Rect, Size};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, Signal};
use zgui::view;
use zgui::view::{AnyView, NodeRef, ScrollBehavior, ScrollTarget};
use zgui_ui::virtualize::Virtualize;

use crate::painted::stage::Stage;

/// Light rows on a black page, so that an edge drawn with less than full coverage shows.
const SHEET: &str = ":root { background-color: #000000; color: #1b1f27; font-family: sans-serif }
                     .app { width: 1280px; height: 800px }
                     .bar { height: 40px; color: #ffffff }
                     .port { height: 704px; overflow-y: scroll }
                     .row { height: 22px; align-items: center; flex-shrink: 0 }
                     .c { width: 90px; padding: 0 6px; white-space: nowrap; overflow: hidden }";

/// How many rows the list has with no filter.
const ROWS: usize = 100_000;

/// How tall one row is, in CSS pixels.
const ROW: f32 = 22.0;

/// The row the port is scrolled to: far enough down that the rows sit 22 000 pixels into the
/// content.
const TOP: usize = 1000;

/// The row tints, picked by key. [`TOP`] is a multiple of their count, so the rows at the top of a
/// port scrolled to it have the tints of the rows at the top of the list.
const TINTS: [&str; 4] = ["#eff3f8", "#f7f7fa", "#f8f4ee", "#fcfcfc"];

/// The tint of the row with `key`.
fn tint(key: usize) -> String {
    TINTS[key % TINTS.len()].to_string()
}

/// Which rows the list holds.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Rows {
    /// None yet, as before the data arrives.
    Empty,
    /// Every key.
    All,
    /// One key in three.
    Filtered,
}

/// The keys the list shows.
fn keys(rows: Rows) -> Vec<usize> {
    match rows {
        Rows::Empty => Vec::new(),
        Rows::All => (0..ROWS).collect(),
        Rows::Filtered => (0..ROWS).filter(|key| key % 3 == 1).collect(),
    }
}

/// The signal the view reads its rows from, handed out once the view is built.
type Switch = Rc<Cell<Option<RwSignal<Rows>>>>;

/// The list under a bar that names the filter, empty until its rows are set.
///
/// The bar's text is empty without a filter. A filter that comes and goes makes the bar's text
/// box come and go, and the boxes after it, the port among them, are built again.
fn list(port: NodeRef, switch: Switch) -> impl Fn() -> AnyView + use<> {
    move || {
        let filter = RwSignal::new(Rows::Empty);
        switch.set(Some(filter));
        let rows = Signal::derive_local(move || keys(filter.get()));
        let count: Signal<usize, LocalStorage> = Signal::derive_local(move || rows.with(Vec::len));
        let seen = Virtualize::new(port, count, ROW, 0);
        let shown = Signal::derive_local(move || {
            let window = seen.window();
            rows.with(|rows| {
                let end = window.end().min(rows.len());
                rows[window.first.min(end)..end].to_vec()
            })
        });
        let named = move || match filter.get() {
            Rows::Filtered => "one in three".to_string(),
            Rows::Empty | Rows::All => String::new(),
        };
        AnyView::new(view! {
            column(class = "app") {
                row(class = "bar") {text {{named}}}
                scroll(class = "port", node_ref = port) {
                    column(
                        style:padding-top = move || Some(format!("{}px", seen.window().lead)),
                        style:padding-bottom = move || Some(format!("{}px", seen.window().trail))
                    ) {
                        for key in move || shown.get(), key = |key: &usize| *key {
                            row(
                                class = "row",
                                style:background-color = move || Some(tint(key))
                            ) {
                                text(class = "c") {{format!("row {}", key % TINTS.len())}}
                            }
                        }
                    }
                }
            }
        })
    }
}

/// Opens the list and gives it `rows`, or reports the run skipped on a machine with no device.
macro_rules! staged {
    ($port:expr, $rows:expr) => {{
        let switch = Switch::default();
        match Stage::open(SHEET, list($port, switch.clone())) {
            Some(mut stage) => {
                stage.resize(1280.0, 800.0);
                let rows = switch.get().expect("the view was built");
                rows.set($rows);
                stage.tick();
                stage.settle();
                (stage, rows)
            }
            None => {
                eprintln!("skipped: no usable graphics device");
                return;
            }
        }
    }};
}

/// Scrolls `port` to row [`TOP`] at once, and runs the frames that asks for.
fn scroll_down(stage: &mut Stage, port: NodeRef) {
    port.scroll_to(
        ScrollTarget::Offset(Point::new(DevicePx(0.0), DevicePx(TOP as f32 * ROW))),
        ScrollBehavior::Instant,
    );
    stage.tick();
    stage.settle();
}

/// The part of the port the rows fill, which leaves out the scrollbar.
fn band(stage: &Stage, port: NodeRef) -> Rect<DevicePx, Device> {
    let rect = stage.rect_of(port.get().expect("the port was built"));
    Rect::new(rect.origin, Size::new(DevicePx(1200.0), rect.size.height))
}

/// Every pixel of `band` in a full repaint.
fn picture(stage: &mut Stage, band: Rect<DevicePx, Device>) -> Vec<(u8, u8, u8)> {
    stage.repaint();
    stage.colours_in(band)
}

/// Fails naming how many pixels of `band` two pictures disagree about, and the first pixel row.
fn assert_same(
    one: &[(u8, u8, u8)],
    two: &[(u8, u8, u8)],
    band: Rect<DevicePx, Device>,
    when: &str,
) {
    assert_eq!(
        one.len(),
        two.len(),
        "{when}: the pictures cover different areas"
    );
    let width = band.size.width.0 as usize;
    let differing: Vec<usize> = one
        .iter()
        .zip(two)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .map(|(index, _)| index)
        .collect();
    assert!(
        differing.is_empty(),
        "{when}: {} pixels differ, the first on pixel row {} ({:?} against {:?})",
        differing.len(),
        differing[0] / width,
        one[differing[0]],
        two[differing[0]],
    );
}

/// A port scrolled 22 000 pixels down draws its rows on the pixels the top of the list uses.
#[test]
fn rows_far_down_a_port_draw_what_rows_at_its_top_draw() {
    let port = NodeRef::new();
    let (mut stage, _) = staged!(port, Rows::All);
    let band = band(&stage, port);
    let top = picture(&mut stage, band);
    scroll_down(&mut stage, port);
    let far = picture(&mut stage, band);
    assert_same(&far, &top, band, "scrolled to row 1000");
}

/// A list filtered and unfiltered again while scrolled far down draws what a fresh window draws.
///
/// The filter builds the port again, so the live window draws its rows on the device while the
/// fresh one, which scrolled, draws them in a content space.
#[test]
fn a_far_scrolled_list_filtered_and_back_draws_what_a_fresh_window_draws() {
    let port = NodeRef::new();
    let (mut stage, rows) = staged!(port, Rows::All);
    let band = band(&stage, port);
    scroll_down(&mut stage, port);
    let first = picture(&mut stage, band);

    rows.set(Rows::Filtered);
    scroll_down(&mut stage, port);
    let narrowed = picture(&mut stage, band);
    assert_ne!(
        narrowed, first,
        "the filter changed nothing, so the fixture asserts nothing"
    );

    rows.set(Rows::All);
    scroll_down(&mut stage, port);
    let back = picture(&mut stage, band);
    drop(stage);

    let fresh = |shown: Rows| {
        let port = NodeRef::new();
        let switch = Switch::default();
        let mut stage =
            Stage::open(SHEET, list(port, switch.clone())).expect("the device was there before");
        stage.resize(1280.0, 800.0);
        switch.get().expect("the view was built").set(shown);
        stage.tick();
        stage.settle();
        scroll_down(&mut stage, port);
        picture(&mut stage, band)
    };
    assert_same(&first, &fresh(Rows::All), band, "after the first scroll");
    assert_same(
        &narrowed,
        &fresh(Rows::Filtered),
        band,
        "with the filter on",
    );
    assert_same(&back, &fresh(Rows::All), band, "with the filter off again");
}
