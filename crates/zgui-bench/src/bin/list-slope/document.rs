//! The document: [`zgui_ui::virtualize::VirtualList`], at a row count and a port height.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use zgui::app::Fonts;
use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{App, AppError, Runtime};
use zgui::view::{Anchor, BuildCx, ClassName};
use zgui::{component, view};
use zgui_bench::reference::watch::{self, Watching};
use zgui_platform_headless::Harness;
use zgui_ui::prelude::*;

/// How wide the window opens, in CSS pixels.
pub(crate) const WIDTH: f32 = 1000.0;

/// How tall one row is, in CSS pixels.
///
/// Declared rather than measured, because that is what a virtualised list is: the window is decided
/// before its rows are built, so a height taken from the rows would mean building all of them to
/// find out which to build.
pub(crate) const ROW: f32 = 24.0;

/// The class name a built row carries, which is how many of them exist is counted.
const ROW_CLASS: &str = "zui-virtual-list__row";

/// The sheet.
///
/// Deliberately plain: a row is a background, a border and two runs of text, so what a scroll costs
/// is the cost of *carrying* rows past a port rather than the cost of whatever the prettiest row in
/// the library does. `.repainted` is the same-run baseline of the glide slope — one class on the
/// list that every realised row's colour sits under, so it reaches exactly the rows the glide moves
/// and nothing else, by a route that has nothing to do with scrolling.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 13px }
     .bench-list { width: 100% }
     .bench-line {
        flex-direction: row;
        height: 24px;
        align-items: center;
        gap: 16px;
        padding: 0 12px;
        background-color: #14161a;
        border-bottom: 1px solid #232833;
     }
     .bench-cell { width: 200px }
     .repainted .bench-line { background-color: #1e2532 }"
);

/// The sheet with the port's height written into it.
///
/// The height is a declaration rather than `100%` because the list is the thing being sized and
/// nothing above it in this document has a height of its own: a percentage against an auto-height
/// ancestor makes the scrollport as tall as its own content, at which point every row is in view and
/// the component under measurement is not virtualising anything. The assertion in
/// [`rows_built`] is what stops that from being a silent measurement of the wrong document.
fn sheet(height: f32) -> String {
    format!("{SHEET}\n.bench-list {{ height: {height}px }}")
}

/// The one class flip that is not a scroll.
pub(crate) type Repaint = RwSignal<bool, LocalStorage>;

/// A hundred thousand rows, of which the port's worth are built.
#[component]
fn LongList(
    /// How many rows the model has.
    rows: usize,
) -> impl IntoView {
    let count = RwSignal::new_local(rows);
    view! {
        VirtualList(
            count = count,
            row_size = ROW,
            label = "Bench",
            class = "bench-list",
            row = move |index: usize| view! { row(class = "bench-line") {
                text(class = "bench-cell") {{move || format!("row {index}")}}
                text(class = "bench-cell") {{move || format!("{}", index * 7 % 977)}}
            } }
        )
    }
}

/// One opened window and everything a measurement reads off it.
pub(crate) struct Opened {
    /// The driven application.
    pub(crate) harness: Harness<Runtime>,
    /// What every drawn frame damaged.
    pub(crate) damage: watch::Log,
    /// The class flip that repaints every realised row without scrolling.
    pub(crate) repaint: Repaint,
}

/// Opens a list of `rows` rows in a window `height` CSS pixels tall.
///
/// The window is the port: the list fills it, so the number of rows that exist is a function of
/// `height` and of nothing else. That is what makes the two sweeps orthogonal — one varies `rows`
/// and holds `height`, the other varies `height` and holds `rows` — and it is why the port is not a
/// style on the list. A list given a height by a rule inside a taller window is a list inside a
/// scroll container inside a scroll container, and the gesture would reach whichever one the
/// pointer was over.
pub(crate) fn opened(rows: usize, height: f32) -> Opened {
    let repaint: Repaint = RwSignal::new_local(false);
    let damage: watch::Log = Rc::new(RefCell::new(Vec::new()));
    let for_renderer = Rc::clone(&damage);
    let render = move |_surface: &std::sync::Arc<dyn zgui::platform::Surface>,
                       target: RenderTarget|
          -> Result<Box<dyn Renderer>, AppError> {
        let mut inner = zgui_testkit_scene::CaptureRenderer::new();
        inner.configure(target);
        let mut watching = Watching::new(Box::new(inner), Rc::clone(&for_renderer));
        watching.configure(target);
        Ok(Box::new(watching))
    };
    // The real font stack, because a wheel notch is measured in *lines* and a line's height is read
    // off the scrolled container's own strut. A window with no text engine behind it resolves that
    // strut to nothing, and every notch delivered to it asks the document to travel zero pixels —
    // a scroll workload that scrolls nothing and reports the cost of not scrolling.
    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts.clone();
    let handler = App::new()
        .with_title("list-slope")
        .with_size(WIDTH, height)
        .with_stylesheet(sheet(height))
        .with_renderer(Box::new(render))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .with_glyph_raster(Box::new(move || raster.raster()))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            let root = zgui::elements::column()
                .class("root")
                .class_toggle(ClassName::new("repainted"), move || repaint.get())
                .child(view! { LongList(rows = rows) });
            Box::new(root.into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(WIDTH),
        DevicePx(height),
    )));
    harness.settle(512);
    for _ in 0..8 {
        harness.advance(Duration::from_micros(16_667));
        harness.pump();
    }
    harness.settle(512);
    damage.borrow_mut().clear();
    Opened {
        harness,
        damage,
        repaint,
    }
}

/// How many rows the list actually built.
///
/// The x-axis of the glide slope, and read off the document rather than computed from the port,
/// because the number that matters is the number of elements that exist. It is also the check that
/// stops the whole workload from measuring nothing: a virtualised list that built no rows scrolls
/// very fast indeed.
pub(crate) fn rows_built(harness: &Harness<Runtime>) -> usize {
    let dom = harness.app().windows()[0].dom();
    let name = ClassName::new(ROW_CLASS);
    let mut found = 0;
    let mut stack = vec![dom.root_node()];
    while let Some(node) = stack.pop() {
        if dom.classes(node).contains(&name) {
            found += 1;
        }
        stack.extend(dom.children(node));
    }
    found
}
