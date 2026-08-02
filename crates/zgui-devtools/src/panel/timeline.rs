//! Where the last frame's time went, as a strip and then as a list.
//!
//! The strip first, because the shape is the answer: one stage occupying most of the width is a
//! different problem from twenty stages of equal size, and no table shows that at a glance. The
//! list under it is the same data in the order the stages ran, with whatever each mark had to say.
//!
//! **Every slice's share of the frame is worked out once, before the strip is built.** A slice's
//! width and whether it is the slow one are both fractions of the whole frame, and asking for the
//! whole frame from inside each slice means summing the stages once per stage — over a vector that
//! has to be cloned out of its signal to be summed. That is quadratic in the number of stages, on
//! the one tab whose entire purpose is to be cheap enough to watch while something else is being
//! measured.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::{component, view};

#[allow(
    unused_imports,
    reason = "the tag names the component and the macro names its props type"
)]
use crate::panel::frame::{Line, LineProps};

use crate::sample::Stage;
use crate::state::DevTools;

/// A stage taking more than this share of the frame is drawn as the slow one.
const SLOW: f64 = 0.25;

/// One stage, with its share of the frame already worked out.
#[derive(Clone, Debug)]
struct Slice {
    /// Where in the frame it ran, which is what keys it.
    at: usize,
    /// The stage itself.
    stage: Stage,
    /// How wide its slice of the strip is.
    width: String,
    /// Whether it took enough of the frame to be drawn as the slow one.
    slow: bool,
}

/// The timeline tab.
#[component]
pub(crate) fn TimelinePanel(
    /// Where the timeline is published.
    tools: DevTools,
) -> impl IntoView {
    let timeline = tools.timeline;
    view! {
        column(class = "zgui-devtools__body") {
            text(class = "zgui-devtools__head") {"the frame before this one, stage by stage"}
            Line(
                name = "whole frame",
                value = move || format!(
                    "{:.1} us",
                    timeline.get().iter().map(|stage| stage.us).sum::<f64>()
                )
            )
            row(class = "zgui-devtools__strip") {
                for slice in move || slices(timeline), key = {|slice: &Slice| slice.at} {
                    box(
                        class = "zgui-devtools__slice",
                        class:zgui-devtools__slice-slow = {slice.slow},
                        style:width = {Some(slice.width.clone())}
                    )
                }
            }
            Show(
                when = move || !timeline.get().is_empty(),
                fallback = || view! {
                    text(class = "zgui-devtools__value-quiet") {
                        "no complete frame recorded yet"
                    }
                }
            ) {
                // Keyed by position: one stage name occurs several times in a frame — a box
                // tree is built once from the frame and again from every observation pass — and
                // that is exactly the shape the strip exists to make visible.
                for slice in move || slices(timeline), key = {|slice: &Slice| slice.at} {
                    row(class = "zgui-devtools__row") {
                        text(class = "zgui-devtools__key") {{slice.stage.name.clone()}}
                        text(class = "zgui-devtools__value") {{format!("{:.1} us", slice.stage.us)}}
                    }
                    Show(when = {let note = slice.stage.note.clone(); move || !note.is_empty()}) {
                        row(class = "zgui-devtools__row") {
                            text(class = "zgui-devtools__key") {""}
                            text(class = "zgui-devtools__value-quiet") {{slice.stage.note.clone()}}
                        }
                    }
                }
            }
        }
    }
}

/// Every stage of the last frame, with its share of the frame already worked out.
///
/// Once per rebuild of the list rather than once per slice, which is the whole point: the share is
/// a fraction of a total that has to be summed, and summing it from inside each slice means
/// cloning the vector out of its signal once per element of it.
fn slices(timeline: RwSignal<Vec<Stage>, LocalStorage>) -> Vec<Slice> {
    let stages = timeline.get();
    let total: f64 = stages.iter().map(|stage| stage.us).sum();
    stages
        .into_iter()
        .enumerate()
        .map(|(at, stage)| Slice {
            at,
            width: width(stage.us, total),
            slow: stage.us > total * SLOW,
            stage,
        })
        .collect()
}

/// How wide one stage's slice is, as a percentage of the strip.
///
/// A floor of a tenth of a percent, because a stage that took no measurable time is still a stage
/// that ran, and a slice of zero width is a stage the strip claims did not happen.
fn width(us: f64, total: f64) -> String {
    let share = if total > 0.0 { us / total } else { 0.0 };
    format!("{:.2}%", (share * 100.0).max(0.1))
}
