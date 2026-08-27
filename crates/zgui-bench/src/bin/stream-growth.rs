//! A diagnostic probe: what minutes of agent streaming leave in the scene's tables.
//!
//! **This is not a CI reference workload.** It answers one report — CPU that climbs to half a
//! core after a few minutes of an agent session, without any interaction — by running the shape
//! of that session: messages appended to a followed scroll thread, the last message's body
//! growing and its ellipsized status line rewritten on every delta, a spinner turning and a
//! glyph breathing the whole time. What is watched is each side table's live count *and its id
//! space*, against the cost of a tick early and late.
//!
//! ```text
//! cargo run --release -p zgui-bench --bin stream-growth
//! ```

#![forbid(unsafe_code)]

use std::time::{Duration, Instant};

use zgui::app::Fonts;
use zgui::geom::{Css, CssPx, DevicePx, Point, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{App, AppError};
use zgui::view::{Anchor, BuildCx, IntoView, View};
use zgui::vocab::{
    Modifiers, PointerAction, PointerEvent, PointerId, PointerKind, ScrollDelta, ScrollPhase,
    Timestamp, WheelEvent,
};
use zgui::{component, view};
use zgui_platform_headless::Harness;
use zgui_profile::{Counter, counter};

/// How many deltas the session streams — minutes of an agent working, at one delta a tick.
const TICKS: usize = 6_000;

/// How many messages the thread can grow to.
const MESSAGES: usize = 200;

/// Every how many ticks a new message begins.
const MESSAGE_EVERY: usize = 40;

/// The agent panel's shapes: a followed thread, ellipsized status lines, wrapped bodies, and the
/// two animations that run while the agent works.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 13px }
     .log { display: block; overflow-y: auto; height: 700px }
     .message { display: flex; flex-direction: column; margin: 4px 0 }
     .head { flex-direction: row; height: 18px; align-items: center; gap: 6px }
     .status { flex: 1 1 auto; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
               color: #8a93a6 }
     .body { display: block }
     @keyframes probe-turn {
         from { transform: rotate(0deg) }
         to { transform: rotate(360deg) }
     }
     @keyframes probe-breathe {
         from { opacity: 1 }
         50% { opacity: 0.35 }
         to { opacity: 1 }
     }
     .spin { width: 12px; height: 12px; background-color: #61afef;
             transform: rotate(0deg);
             animation-name: probe-turn; animation-duration: 900ms;
             animation-timing-function: linear; animation-iteration-count: infinite }
     .breathe { animation-name: probe-breathe; animation-duration: 1400ms;
                animation-timing-function: ease-in-out; animation-iteration-count: infinite }"
);

/// One message's changing text.
#[derive(Clone)]
struct Message {
    /// The one-line status, rewritten on every delta while the message runs.
    status: RwSignal<String, LocalStorage>,
    /// The body, appended to on every delta while the message runs.
    body: RwSignal<String, LocalStorage>,
}

/// The thread: every begun message, a spinner and a breathing glyph at the foot.
#[component]
fn Thread(
    /// How many messages have begun.
    count: RwSignal<usize, LocalStorage>,
    /// Every message there will ever be.
    messages: Vec<Message>,
) -> impl IntoView {
    view! {
        scroll(class = "log") {
            column {
                for index in move || 0..count.get(), key = |index: &usize| *index {
                    {message_view(messages[index].clone(), index)}
                }
                row(class = "head") {
                    box(class = "spin") {}
                    label(class = "breathe") {{"working"}}
                }
            }
        }
    }
}

/// One message: an ellipsized head line over a wrapped body.
fn message_view(message: Message, index: usize) -> impl IntoView {
    view! {
        column(class = "message") {
            row(class = "head") {
                label {{format!("step {index}")}}
                label(class = "status") {{move || message.status.get()}}
            }
            text(class = "body") {{move || message.body.get()}}
        }
    }
}

/// The middle of the port, which is where the wheel points.
fn middle() -> Point<CssPx, Css> {
    Point::new(CssPx(450.0), CssPx(350.0))
}

/// One wheel notch of `lines`, which is what keeps the thread followed to its foot.
fn notch(lines: f32) -> SurfaceEvent {
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

fn main() {
    println!(
        "This is a diagnostic probe and not a reference workload: it streams an agent-shaped \
         session into a followed thread and watches what the scene's tables keep."
    );

    let messages: Vec<Message> = (0..MESSAGES)
        .map(|_| Message {
            status: RwSignal::new_local(String::new()),
            body: RwSignal::new_local(String::new()),
        })
        .collect();
    let count = RwSignal::new_local(1_usize);
    let mounted = messages.clone();

    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts.clone();
    let handler = App::new()
        .with_title("stream-growth")
        .with_size(900.0, 700.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(
            |_surface: &std::sync::Arc<dyn zgui::platform::Surface>,
             target: RenderTarget|
             -> Result<Box<dyn Renderer>, AppError> {
                let mut inner = zgui_testkit_scene::CaptureRenderer::new();
                inner.configure(target);
                Ok(Box::new(inner))
            },
        ))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .with_glyph_raster(Box::new(move || raster.raster()))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(
                view! { Thread(count = count, messages = mounted.clone()) }
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(900.0),
        DevicePx(700.0),
    )));
    harness.deliver_to_first(SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(middle()),
        modifiers: Modifiers::default(),
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(256);
    counter::reset();

    println!(
        "{:>6} {:>10} {:>10} {:>11} {:>11} {:>8} {:>9}",
        "tick", "clips", "clip_ids", "paints", "paint_ids", "spaces", "cost_us"
    );
    let words = ["reading", "grepping", "building", "measuring", "writing"];
    let mut costs: Vec<f64> = Vec::new();
    for tick in 0..TICKS {
        let sampled = tick == 200 || tick + 100 == TICKS;
        let mark = sampled.then(zgui_profile::counter::snapshot);
        let current = count.get_untracked().min(MESSAGES) - 1;
        if tick % MESSAGE_EVERY == MESSAGE_EVERY - 1 && count.get_untracked() < MESSAGES {
            count.update(|held| *held += 1);
        }
        let word = words[tick % words.len()];
        messages[current].status.set(format!(
            "{word} step {tick} of the long streaming session, cut off somewhere past the edge"
        ));
        messages[current]
            .body
            .update(|held| held.push_str(&format!(" {word} more output arrives here")));
        // The follow: pinned to the foot the way the agent panel keeps itself.
        harness.deliver_to_first(notch(40.0));
        let started = Instant::now();
        harness.advance(Duration::from_millis(16));
        harness.pump();
        harness.settle(16);
        costs.push(started.elapsed().as_secs_f64() * 1e6);
        if let Some(mark) = mark {
            let moved = mark.delta(&zgui_profile::counter::snapshot());
            println!(
                "tick {tick}: shaped={} rebroken={} relaid={} rebuilt={} diffed={} reencoded={} \
                 translated={} emitted={} glyphs={} restyled={}",
                moved.text_shaped,
                moved.text_rebroken,
                moved.nodes_relaid_out,
                moved.boxes_rebuilt,
                moved.fragments_diffed,
                moved.chunks_reencoded,
                moved.chunks_translated,
                moved.primitives_emitted,
                moved.glyphs_placed,
                moved.elements_restyled,
            );
        }
        if tick % 500 == 0 || tick + 1 == TICKS {
            costs.sort_by(f64::total_cmp);
            let p50 = costs[costs.len() / 2];
            println!(
                "{tick:>6} {:>10} {:>10} {:>11} {:>11} {:>8} {p50:>9.1}",
                counter::get(Counter::ClipEntriesLive),
                counter::get(Counter::ClipSlotsReach),
                counter::get(Counter::PaintEntriesLive),
                counter::get(Counter::PaintSlotsReach),
                counter::get(Counter::SpatialNodesLive),
                p50 = p50,
            );
            costs.clear();
        }
    }

    // The other half of the report: the agent stops streaming and the window is left alone, so
    // the only motion is the spinner and the breathing glyph. What one of *those* frames costs on
    // the grown thread is what an untouched window burns for as long as it stands.
    println!("quiet phase: animations only, over the grown thread");
    costs.clear();
    for tick in 0..1_000_usize {
        let sampled = tick == 900;
        let mark = sampled.then(zgui_profile::counter::snapshot);
        let started = Instant::now();
        harness.advance(Duration::from_millis(8));
        harness.pump();
        harness.settle(16);
        costs.push(started.elapsed().as_secs_f64() * 1e6);
        if let Some(mark) = mark {
            let moved = mark.delta(&zgui_profile::counter::snapshot());
            println!(
                "quiet tick {tick}: shaped={} rebroken={} relaid={} rebuilt={} diffed={} \
                 reencoded={} translated={} emitted={} glyphs={} restyled={}",
                moved.text_shaped,
                moved.text_rebroken,
                moved.nodes_relaid_out,
                moved.boxes_rebuilt,
                moved.fragments_diffed,
                moved.chunks_reencoded,
                moved.chunks_translated,
                moved.primitives_emitted,
                moved.glyphs_placed,
                moved.elements_restyled,
            );
        }
        if tick % 250 == 0 || tick + 1 == 1_000 {
            costs.sort_by(f64::total_cmp);
            let p50 = costs[costs.len() / 2];
            println!("quiet {tick:>5} cost_us p50 {p50:>9.1}");
            costs.clear();
        }
    }
    harness.shut_down();
}
