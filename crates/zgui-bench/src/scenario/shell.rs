//! App shell: tooltips, a dock dragged taller and shorter, and the window resized, over one page.
//!
//! The shape of a real tool rather than a gallery: a head of icon buttons that each name
//! themselves in a tooltip, a page of rows built from library controls, and a dock below it in a
//! vertical resizable group, holding a long virtualised log. The three interactions a person does
//! most in such a window each have a cost that has to follow what changed, and each one has a
//! count beside its time that says whether it does.
//!
//! * **Tooltips.** Opening one mounts a small surface on an overlay band. What it costs is that
//!   surface, never the page: the count is whole-document box-tree builds, which must stay zero.
//! * **Dock drag.** One step moves one edge. The panels' sizes reach the layout, and the page's
//!   rows are moved or clipped; none of their styles changes, so the restyle count is a handful,
//!   the hit index is never rebuilt and the observers settle in one pass.
//! * **Resize.** Every observer delivers once and settles inside the frame, so no delivery is
//!   ever cut short and left to a second frame.

use std::time::Duration;

use zgui::geom::{Css, CssPx, DevicePx, Point, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::runtime::Runtime;
use zgui::view::{IntoView, View, ViewHost};
use zgui::vocab::{Modifiers, PointerAction, PointerButton, PointerEvent, Timestamp};
use zgui::{component, view};
use zgui_platform_headless::Harness;
use zgui_ui::prelude::*;
use zgui_ui_primitives::Orientation;
use zgui_ui_tokens::prelude::*;

use crate::scenario::band::{Band, INTERACTION_TOLERANCE, Measurement, Pace, Spread};
use crate::scenario::{Outcome, counters, fixture, quiet};

/// How many icon buttons the head holds, each with a tooltip.
const TRIGGERS: usize = 12;

/// How many rows the page holds.
const ROWS: usize = 48;

/// How many lines the dock's log holds.
const LINES: usize = 5_000;

/// How tall one log line is.
const LINE_HEIGHT: f32 = 18.0;

/// How many times the pointer sweeps the head.
const SWEEPS: usize = 3;

/// How many steps one drag takes, and how far each one moves the edge.
const DRAG_STEPS: usize = 60;

/// How far one drag step moves the dock's edge, in CSS pixels.
const DRAG_STEP: f32 = 5.0;

/// How many window sizes the resize phase steps through.
const RESIZE_STEPS: usize = 40;

/// The page's own rules. The library controls bring theirs.
const SHEET: &str = zgui::css!(
    ".shell { width: 100%; height: 100%; flex-direction: column }
     .shell-head { flex-direction: row; gap: 4px; padding: 4px 8px; flex: 0 0 auto }
     .shell-page { flex-direction: column; overflow: hidden; flex: 1 1 auto }
     .shell-row {
        flex-direction: row;
        align-items: center;
        gap: 12px;
        height: 28px;
        padding: 0 12px;
        flex: 0 0 auto;
     }
     .shell-row:hover { background-color: var(--zui-muted) }
     .shell-cell { width: 160px }
     .shell-log { height: 100% }
     .shell-line { height: 18px; padding: 0 8px; font-family: monospace }"
);

/// One row of the page: a badge, text cells and a ghost button, all library controls.
#[component]
fn ShellRow(
    /// Which row this is.
    index: usize,
) -> impl IntoView {
    view! {
        row(class = "shell-row") {
            Badge(variant = BadgeVariant::Secondary) {{format!("ns-{}", index % 7)}}
            text(class = "shell-cell") {{format!("workload-{index}")}}
            text(class = "shell-cell") {{format!("{}m", index * 3 % 59)}}
            text(class = "shell-cell") {"Running"}
            Button(variant = ButtonVariant::Ghost, size = ButtonSize::Sm) {"Logs"}
        }
    }
}

/// The whole window.
#[component]
fn Shell() -> impl IntoView {
    let scheme = RwSignal::new_local(ColorScheme::Dark);
    let lines = RwSignal::new_local(LINES);
    view! {
        ThemeProvider(scheme = scheme) {
            TooltipProvider(delay = Duration::ZERO, close_delay = Duration::ZERO) {
                column(class = "shell") {
                    row(class = "shell-head") {
                        for index in move || 0..TRIGGERS, key = |index: &usize| *index {
                            Tooltip {
                                TooltipTrigger {
                                    Button(
                                        variant = ButtonVariant::Ghost,
                                        size = ButtonSize::Icon,
                                        attr:data-testid = Some(format!("tip-{index}"))
                                    ) {{format!("{index}")}}
                                }
                                TooltipContent {{format!("Action number {index}")}}
                            }
                        }
                    }
                    ResizablePanelGroup(direction = Orientation::Vertical) {
                        ResizablePanel(default_size = 65.0, min_size = 10.0) {
                            column(class = "shell-page") {
                                for index in move || 0..ROWS, key = |index: &usize| *index {
                                    ShellRow(index = index)
                                }
                            }
                        }
                        ResizableHandle(attr:data-testid = "dock-handle")
                        ResizablePanel(default_size = 35.0, min_pixels = 120.0) {
                            VirtualList(
                                count = lines,
                                row_size = LINE_HEIGHT,
                                label = "Log",
                                class = "shell-log",
                                row = move |index: usize| view! {
                                    text(class = "shell-line") {{format!(
                                        "2026-10-09T12:00:{:02}Z line {index} request served", index % 60
                                    )}}
                                }
                            )
                        }
                    }
                }
            }
        }
    }
}

/// Builds the runtime holding the shell.
fn runtime() -> Runtime {
    fixture::custom(SHEET, |cx| {
        Box::new(view! { Shell() }.into_view().build(cx))
    })
}

/// The centre of the element carrying `data-testid = name`, in CSS pixels.
///
/// # Panics
///
/// Panics when nothing carries the name, because every phase below aims at one and a pointer
/// aimed at nothing measures nothing.
fn centre(harness: &Harness<Runtime>, name: &str) -> Point<CssPx, Css> {
    let window = &harness.app().windows()[0];
    let attribute = zgui::view::AttrName::new("data-testid");
    let dom = window.dom();
    let mut stack = vec![dom.root_node()];
    let mut found = None;
    while let Some(node) = stack.pop() {
        if dom.attribute(node, attribute).as_deref() == Some(name) {
            found = window.host().window_box(node);
            break;
        }
        stack.extend(dom.children(node));
    }
    let rect = found.unwrap_or_else(|| panic!("the shell has no element named `{name}`"));
    let scale = window.scale().get();
    Point::new(
        CssPx((rect.origin.x.0 + rect.size.width.0 / 2.0) / scale),
        CssPx((rect.origin.y.0 + rect.size.height.0 / 2.0) / scale),
    )
}

/// Delivers one pointer event and settles, and answers how long that took in microseconds.
fn timed(harness: &mut Harness<Runtime>, event: SurfaceEvent) -> f64 {
    let started = std::time::Instant::now();
    harness.deliver_to_first(event);
    harness.settle(64);
    started.elapsed().as_secs_f64() * 1e6
}

/// Lets every running transition and animation finish.
fn idle(harness: &mut Harness<Runtime>, frames: usize) {
    for _ in 0..frames {
        harness.advance(Duration::from_micros(16_667));
        harness.pump();
    }
}

/// What the tooltip phase measured.
struct Tooltips {
    /// What one crossing onto a trigger cost, with the frames that animate its tooltip in and the
    /// last one out, in microseconds.
    crossings: Vec<f64>,
    /// Whole-document box-tree builds over the phase.
    builds: u64,
    /// Hit-index rebuilds over the phase.
    hit_rebuilds: u64,
}

/// Sweeps the pointer across the head's triggers, resting on each until its tooltip is up.
fn tooltips(harness: &mut Harness<Runtime>) -> Tooltips {
    let aims: Vec<_> = (0..TRIGGERS)
        .map(|index| centre(harness, &format!("tip-{index}")))
        .collect();
    let mut crossings = Vec::new();
    harness.deliver_to_first(crate::input::pointer(PointerAction::Entered, aims[0]));
    harness.settle(64);
    let before = zgui_profile::counter::snapshot();
    for _ in 0..SWEEPS {
        for &at in &aims {
            let started = std::time::Instant::now();
            harness.deliver_to_first(crate::input::pointer(PointerAction::Moved, at));
            harness.settle(64);
            idle(harness, 6);
            crossings.push(started.elapsed().as_secs_f64() * 1e6);
        }
    }
    // Off the head, so the last tooltip closes and unmounts inside the measurement.
    let away = Point::new(CssPx(crate::gallery::WIDTH / 2.0), CssPx(200.0));
    crossings.push(timed(
        harness,
        crate::input::pointer(PointerAction::Moved, away),
    ));
    idle(harness, 6);
    let moved = before.delta(&zgui_profile::counter::snapshot());
    Tooltips {
        crossings,
        builds: moved.box_tree_builds,
        hit_rebuilds: moved.hit_index_rebuilds,
    }
}

/// A pointer event carrying the primary button, as a press and a release do.
fn primary(action: PointerAction, at: Point<CssPx, Css>) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(at).with_button(PointerButton::Primary),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// What the drag phase measured.
struct Drag {
    /// What one drag step cost, in microseconds.
    steps: Vec<f64>,
    /// Elements restyled or recascaded per step.
    restyled: f64,
    /// Primitives emitted per step.
    emitted: f64,
    /// Hit-index rebuilds over the phase.
    hit_rebuilds: u64,
    /// Observation passes per step.
    passes: f64,
}

/// Drags the dock's edge up and back down.
fn drag(harness: &mut Harness<Runtime>) -> Drag {
    let start = centre(harness, "dock-handle");
    let _ = timed(harness, crate::input::pointer(PointerAction::Moved, start));
    let _ = timed(harness, primary(PointerAction::Pressed, start));
    let before = zgui_profile::counter::snapshot();
    let mut steps = Vec::with_capacity(DRAG_STEPS);
    let mut y = start.y.0;
    for step in 0..DRAG_STEPS {
        y += if step < DRAG_STEPS / 2 {
            -DRAG_STEP
        } else {
            DRAG_STEP
        };
        steps.push(timed(
            harness,
            crate::input::pointer(PointerAction::Moved, Point::new(start.x, CssPx(y))),
        ));
        harness.advance(Duration::from_micros(8_333));
    }
    let moved = before.delta(&zgui_profile::counter::snapshot());
    let _ = timed(
        harness,
        primary(PointerAction::Released, Point::new(start.x, CssPx(y))),
    );
    #[expect(
        clippy::cast_precision_loss,
        reason = "counts here are in the thousands"
    )]
    let per = |total: u64| total as f64 / DRAG_STEPS as f64;
    assert!(
        moved.nodes_relaid_out > 0,
        "{DRAG_STEPS} drag steps laid nothing out, so the pointer never held the handle"
    );
    Drag {
        steps,
        restyled: per(moved.elements_restyled + moved.elements_recascaded),
        emitted: per(moved.primitives_emitted),
        hit_rebuilds: moved.hit_index_rebuilds,
        passes: per(moved.observation_passes),
    }
}

/// What the resize phase measured.
struct Resize {
    /// What one window size cost, in microseconds.
    steps: Vec<f64>,
    /// Deliveries cut short over the phase.
    truncated: u64,
}

/// Steps the window through a range of sizes.
fn resize(harness: &mut Harness<Runtime>) -> Resize {
    let before = zgui_profile::counter::snapshot();
    let mut steps = Vec::with_capacity(RESIZE_STEPS);
    for step in 0..RESIZE_STEPS {
        #[expect(clippy::cast_precision_loss, reason = "the step index is tens")]
        let shrink = (step % 20) as f32 * 16.0;
        let size = Size::new(
            DevicePx(crate::gallery::WIDTH - shrink),
            DevicePx(crate::gallery::HEIGHT - shrink / 2.0),
        );
        let started = std::time::Instant::now();
        harness.deliver_to_first(SurfaceEvent::Resized(size));
        harness.settle(64);
        steps.push(started.elapsed().as_secs_f64() * 1e6);
        harness.advance(Duration::from_micros(16_667));
        harness.pump();
    }
    let moved = before.delta(&zgui_profile::counter::snapshot());
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(crate::gallery::WIDTH),
        DevicePx(crate::gallery::HEIGHT),
    )));
    harness.settle(64);
    Resize {
        steps,
        truncated: moved.observations_truncated,
    }
}

/// Runs the scenario.
pub(crate) fn run() -> Outcome {
    let mut harness = crate::drive::harness(runtime());
    quiet(&mut harness);
    let all_before = zgui_profile::counter::snapshot();

    let tips = tooltips(&mut harness);
    let dragged = drag(&mut harness);
    let resized = resize(&mut harness);
    let all = all_before.delta(&zgui_profile::counter::snapshot());

    let mut crossings = tips.crossings;
    let mut drag_steps = dragged.steps;
    let mut resize_steps = resized.steps;
    let pace = Pace::of(&drag_steps, 8_333.0);
    let crossing = Spread::of(&mut crossings);
    let step = Spread::of(&mut drag_steps);
    let size = Spread::of(&mut resize_steps);

    #[expect(clippy::cast_precision_loss, reason = "counts here are small")]
    let count = |value: u64| value as f64;
    Outcome {
        scenario: "app-shell",
        document: format!(
            "{TRIGGERS} tooltip triggers, {ROWS} rows of library controls, a dock of {LINES} \
             virtualised lines in a vertical resizable group; {SWEEPS} sweeps, {DRAG_STEPS} drag \
             steps of {DRAG_STEP} px, {RESIZE_STEPS} window sizes"
        ),
        measurements: vec![
            Measurement {
                name: "shell.tooltip_crossing",
                unit: "us",
                value: crossing.p50,
                band: Band::Time {
                    baseline: 1_250.0,
                    tolerance: INTERACTION_TOLERANCE,
                },
                rationale: "measured at 1.24 ms; one tooltip mounted on its band and the last \
                            one handed over, and the page built again for neither (10-11 ms \
                            while every mount rebuilt the document's boxes)",
                budget: Some(4_000.0),
                spread: Some(crossing),
            },
            Measurement {
                name: "shell.tooltip_tree_builds",
                unit: "builds",
                value: count(tips.builds),
                band: Band::Count { ceiling: 0 },
                rationale: "a tooltip is a small surface on its own band; a whole box-tree build \
                            renames every box in the window for it",
                budget: Some(0.0),
                spread: None,
            },
            Measurement {
                name: "shell.tooltip_hit_rebuilds",
                unit: "rebuilds",
                value: count(tips.hit_rebuilds),
                band: Band::Count { ceiling: 5 },
                rationale: "a surface born on a band its parent paints in stacking order rather \
                            than layout order renumbers the index; the sweep measured five",
                budget: None,
                spread: None,
            },
            Measurement {
                name: "shell.drag_step",
                unit: "us",
                value: step.p50,
                band: Band::Time {
                    baseline: 300.0,
                    tolerance: INTERACTION_TOLERANCE,
                },
                rationale: "measured at 0.30 ms; one edge moved: two panels sized, the strips \
                            along the edge repainted, the log's newly exposed lines built",
                budget: Some(8_333.0),
                spread: Some(step),
            },
            Measurement {
                name: "shell.drag_restyled",
                unit: "elems",
                value: dragged.restyled,
                band: Band::Count { ceiling: 8 },
                rationale: "a panel's size is a basis on the panel, which no descendant inherits",
                budget: None,
                spread: None,
            },
            Measurement {
                name: "shell.drag_hit_rebuilds",
                unit: "rebuilds",
                value: count(dragged.hit_rebuilds),
                band: Band::Count { ceiling: 0 },
                rationale: "moving boxes keeps their paint order, so the hit index moves its \
                            entries in place",
                budget: None,
                spread: None,
            },
            Measurement {
                name: "shell.drag_observation_passes",
                unit: "passes",
                value: dragged.passes,
                band: Band::Count { ceiling: 2 },
                rationale: "the log's scrollport is delivered once, and one more pass finds \
                            nothing left to deliver; a third is an observer that did not settle",
                budget: None,
                spread: None,
            },
            Measurement {
                name: "shell.drag_primitives_emitted",
                unit: "prims",
                value: dragged.emitted,
                band: Band::Count { ceiling: 1_000 },
                rationale: "measured at 864: the strips along the moved edge and the log's \
                            lines, never the page above it",
                budget: None,
                spread: None,
            },
            Measurement {
                name: "shell.resize_step",
                unit: "us",
                value: size.p50,
                band: Band::Time {
                    baseline: 2_000.0,
                    tolerance: INTERACTION_TOLERANCE,
                },
                rationale: "measured at 1.2-2.1 ms; every box is asked again at a new size, \
                            once, in one frame, and only what moved and what is exposed is \
                            painted",
                budget: None,
                spread: Some(size),
            },
            Measurement {
                name: "shell.resize_truncated",
                unit: "deliveries",
                value: count(resized.truncated),
                band: Band::Count { ceiling: 0 },
                rationale: "a delivery cut short paints a frame against geometry its observers \
                            have not answered, and asks for another",
                budget: Some(0.0),
                spread: None,
            },
        ]
        .into_iter()
        .chain(crate::scenario::band::whole_document_reshape(&all))
        .collect(),
        counters: counters(&all),
        notes: Vec::new(),
        pace,
    }
}
