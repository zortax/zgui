//! The document: a long form of identical controls, and the two signals that change it.

use std::time::Duration;

use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{App, AppError, Runtime};
use zgui::view::{Anchor, BuildCx, ClassName};
use zgui_platform_headless::Harness;

/// How wide the window opens, in CSS pixels.
const WIDTH: f32 = 1000.0;

/// How tall it opens.
const HEIGHT: f32 = 800.0;

/// The sheet.
///
/// `--hot` and `--warm` change the same declaration, `background-color`, by two different routes:
/// `.hot` is a class on one control and `.warm` is a class on the root that every control's own
/// rule sits under. That is the whole design of the workload — one property, two reaches — and it
/// is why the two rules are written next to each other and give the same colour. A pair of rules
/// that changed different properties would be comparing two costs rather than two reaches.
const SHEET: &str = "root { display: block; width: 100%; height: 100%; overflow: scroll }
                     .cell {
                        display: block;
                        width: 60%;
                        height: 16px;
                        margin: 2px 0;
                        padding: 0 6px;
                        border: 1px solid rgb(52, 58, 74);
                        border-radius: 4px;
                        background-color: rgb(30, 34, 44);
                        color: rgb(214, 222, 235);
                     }
                     .cell:hover { border-color: rgb(96, 108, 136) }
                     .cell:focus { border-color: rgb(126, 227, 255) }
                     .cell.hot { background-color: rgb(47, 107, 255) }
                     .warm .cell { background-color: rgb(47, 107, 255) }";

/// The two class flips the workload is made of.
#[derive(Clone, Copy)]
pub(crate) struct Signals {
    /// Whether the first control carries `.hot` — the local change.
    pub(crate) hot: RwSignal<bool, LocalStorage>,
    /// Whether the root carries `.warm` — the whole-document change.
    pub(crate) warm: RwSignal<bool, LocalStorage>,
}

impl Signals {
    /// A fresh pair, both off.
    pub(crate) fn new() -> Self {
        Self {
            hot: RwSignal::new_local(false),
            warm: RwSignal::new_local(false),
        }
    }
}

/// A renderer that records the display list and draws nowhere.
///
/// The measurement is of the pipeline that *produces* a frame — restyle, layout, paint, encode —
/// and a real graphics device would add a submission and a present whose cost is the driver's
/// rather than this workspace's, on both halves of the ratio and in different proportions.
fn capture(
    _surface: &std::sync::Arc<dyn zgui::platform::Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    let mut renderer = zgui_testkit_scene::CaptureRenderer::new();
    renderer.configure(target);
    Ok(Box::new(renderer))
}

/// Opens a document of `controls` controls, settled and warm.
pub(crate) fn opened(controls: usize, signals: Signals) -> Harness<Runtime> {
    let handler = App::new()
        .with_title("static-slope")
        .with_size(WIDTH, HEIGHT)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(capture))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            let hot = signals.hot;
            let mut root = zgui::elements::column()
                .class("root")
                .class_toggle(ClassName::new("warm"), move || signals.warm.get());
            for index in 0..controls {
                let mut cell = zgui::elements::control().class("cell");
                // Exactly one control in the document follows the local signal, and it is the first
                // — which is on screen at every size, so the change it makes is one the paint stage
                // has to answer rather than one a scrollport throws away.
                if index == 0 {
                    cell = cell.class_toggle(ClassName::new("hot"), move || hot.get());
                }
                root = root.child(cell);
            }
            Box::new(root.into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(WIDTH),
        DevicePx(HEIGHT),
    )));
    harness.settle(512);
    for _ in 0..8 {
        harness.advance(Duration::from_micros(16_667));
        harness.pump();
    }
    harness.settle(512);
    harness
}

/// How many controls the document actually laid out, read off the layout rather than assumed.
///
/// The check this feeds is the one that stops the slope from being taken against a number that is
/// not the number in the document: a builder that quietly stopped at the first thousand would leave
/// four identical documents and a slope of zero, which under a ceiling is a pass.
pub(crate) fn controls_built(harness: &Harness<Runtime>) -> usize {
    let window = &harness.app().windows()[0];
    let dom = window.dom();
    let name = ClassName::new("cell");
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
