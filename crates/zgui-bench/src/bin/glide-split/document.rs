//! The document: one rigid scroller whose every row exists.
//!
//! The same markup, the same sheet and the same wheel gesture as `unvirtualised-probe`, because the
//! walk this probe splits is the walk that document exists to provoke: nothing in a scrolled
//! container is restyled or relaid out, every piece of it is the same piece one offset further
//! along, and the offsetting walk therefore reaches every box in it on every frame of the glide.
//!
//! A virtualised list would not answer the question at all. It mounts the rows in front of the
//! port, so the walk is over tens of boxes whatever the model holds, and a split of a cost that
//! does not grow is a split of nothing.
//!
//! # Why the rows are in groups
//!
//! A moved subtree is timed as a whole, and the smallest unit the scroller offers is whatever its
//! own children are: a flat list of rows is one moved subtree per row, which is seven boxes — a few
//! hundred nanoseconds, bracketed by two reads of a clock that costs tens. The measurement would be
//! mostly clock. Grouping the rows into sections makes each moved subtree a section, three orders
//! of magnitude larger than the bracket around it, and changes nothing about the walk: the same
//! boxes are reached, in the same order, doing the same work. Documents have sections anyway.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use zgui::app::Fonts;
use zgui::geom::{Css, CssPx, DevicePx, Point, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{App, AppError, Runtime};
use zgui::view::{Anchor, BuildCx};
use zgui::vocab::{
    Modifiers, PointerAction, PointerEvent, PointerId, PointerKind, ScrollDelta, ScrollPhase,
    Timestamp, WheelEvent,
};
use zgui_bench::reference::watch::{self, Watching};
use zgui_platform_headless::Harness;

/// How wide the window opens, in CSS pixels.
const WIDTH: f32 = 1000.0;

/// How tall it opens, which is also the port.
const HEIGHT: f32 = 800.0;

/// How many rows are in one section.
///
/// Large enough that the clock around a moved subtree is noise beside it, and small enough that a
/// document of a few thousand rows still has sections rather than being one.
const GROUP: usize = 250;

/// The sheet: a plain scroll container of plain rows, in sections.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 13px }
     .bench-list { width: 100%; height: 800px; overflow: scroll; flex-direction: column }
     .bench-line {
        flex-direction: row;
        height: 24px;
        align-items: center;
        gap: 16px;
        padding: 0 12px;
        background-color: #14161a;
        border-bottom: 1px solid #232833;
     }
     .bench-group { flex-direction: column }
     .bench-cell { width: 200px }"
);

/// Whether the rows clip what they contain.
///
/// A clipping box is the only box that re-interns a clip chain as the walk passes through it, and
/// the plain document has exactly one — the scroller itself, which is composed rather than moved.
/// The variant where every row clips is what says what that interning costs, because it is the
/// difference between two documents that are otherwise the same.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Clipping {
    /// Only the scroller clips, which is what an ordinary list looks like.
    Scroller,
    /// Every row clips its own contents as well.
    EveryRow,
}

impl Clipping {
    /// How the document reports itself.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Clipping::Scroller => "plain",
            Clipping::EveryRow => "clipping",
        }
    }

    /// What it adds to the sheet.
    fn rule(self) -> &'static str {
        match self {
            Clipping::Scroller => "",
            Clipping::EveryRow => "\n.bench-line { overflow: hidden }",
        }
    }
}

/// One opened scroller and the log of what its frames damaged.
pub(crate) struct Opened {
    /// The driven application.
    pub(crate) harness: Harness<Runtime>,
    /// What every drawn frame damaged, which is how frames drawn are counted.
    pub(crate) damage: watch::Log,
}

/// Opens a document of `rows` rows, every one of them an element, and settles it.
pub(crate) fn opened(rows: usize, clipping: Clipping) -> Opened {
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
    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts.clone();
    let handler = App::new()
        .with_title("glide-split")
        .with_size(WIDTH, HEIGHT)
        .with_stylesheet(format!("{SHEET}{}", clipping.rule()))
        .with_renderer(Box::new(render))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .with_glyph_raster(Box::new(move || raster.raster()))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            let mut list = zgui::elements::column().class("bench-list");
            for start in (0..rows).step_by(GROUP) {
                let mut group = zgui::elements::column().class("bench-group");
                for index in start..(start + GROUP).min(rows) {
                    group = group.child(
                        zgui::elements::row()
                            .class("bench-line")
                            .child(
                                zgui::elements::text()
                                    .class("bench-cell")
                                    .child(format!("row {index}")),
                            )
                            .child(
                                zgui::elements::text()
                                    .class("bench-cell")
                                    .child(format!("{}", index * 7 % 977)),
                            ),
                    );
                }
                list = list.child(group);
            }
            Box::new(list.into_view().build(cx))
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
    // The pointer has to be over the scroller before a wheel event can reach it.
    harness.deliver_to_first(SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(middle()),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(64);
    damage.borrow_mut().clear();
    Opened { harness, damage }
}

/// The middle of the port.
pub(crate) fn middle() -> Point<CssPx, Css> {
    Point::new(CssPx(WIDTH / 2.0), CssPx(HEIGHT / 2.0))
}

/// One wheel notch of `lines` lines.
pub(crate) fn notch(lines: f32) -> SurfaceEvent {
    SurfaceEvent::Wheel {
        event: WheelEvent {
            delta: ScrollDelta::Lines { x: 0.0, y: lines },
            phase: ScrollPhase::Discrete,
            position: middle(),
            id: PointerId::MOUSE,
            kind: PointerKind::Mouse,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// How many boxes the document laid out.
pub(crate) fn boxes(harness: &Harness<Runtime>) -> usize {
    harness.app().windows()[0].layout().borrow().keys().len()
}
