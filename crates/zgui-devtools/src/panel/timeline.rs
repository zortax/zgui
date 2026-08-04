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
use zgui::view::NodeRef;
use zgui::{component, view};

#[allow(
    unused_imports,
    reason = "the tag names the component and the macro names its props type"
)]
use crate::panel::frame::{Line, LineProps};

use crate::panel::stage::{Category, describe};
use crate::sample::Stage;
use crate::state::DevTools;

/// A stage taking more than this share of the frame is drawn as the slow one.
const SLOW: f64 = 0.25;

/// What a full-height bar in the frame-time chart means, in microseconds.
///
/// One refresh at 60 Hz. A fixed scale rather than the tallest bar in the window, because the
/// question a frame-time chart answers is "did this frame fit" — and a chart normalised to its own
/// worst sample says nothing about that at all: a run of perfect frames and a run of terrible ones
/// draw exactly the same picture.
const BUDGET: f64 = 16_667.0;

/// The tallest frame the graph plots, as a multiple of the budget.
///
/// A frame four refreshes long reaches the top. Beyond that the line is clamped and the number
/// beside the graph is the honest answer — a scale that grew to fit its worst sample would redraw
/// the whole history every time one frame stuttered, and flatten everything else while it did.
const CEILING: f64 = 4.0;

/// One stage, with its share of the frame already worked out.
#[derive(Clone, Debug)]
struct Slice {
    /// Where in the frame it ran, which is what keys it.
    at: usize,
    /// The stage itself.
    stage: Stage,
    /// What the stage did, in words, or the raw mark when this build has none for it.
    label: String,
    /// The raw mark name, shown beside the label so the source stays findable.
    ///
    /// Empty when the label *is* the raw name, because showing it twice says nothing.
    raw: String,
    /// Which part of the pipeline it belongs to, which is what colours it.
    category: Category,
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
    let history = tools.history;
    // The graph draws in its own pixels, so it has to know how many it got. Observed rather than
    // read once: the panel is resizable, and a plot built against last frame's width is a line that
    // stops short of the edge until something else redraws it.
    let plot = NodeRef::new();
    let extent = plot.observe_border_box();
    view! {
        column(class = "zgui-devtools__body") {
            Line(
                name = "whole frame",
                value = move || format!(
                    "{:.1} us",
                    timeline.get().iter().map(|stage| stage.us).sum::<f64>()
                )
            )
            // The frames before this one, as a line. The strip below says where *a* frame's time
            // went; this says which frames were expensive — and the expensive one is never the one
            // the panel happens to be showing, so a breakdown alone cannot answer it.
            Show(when = move || !history.get().is_empty()) {
                text(class = "zgui-devtools__head") {"the last half minute, frame by frame"}
                row(class = "zgui-devtools__plot") {
                    // The scale, so a peak is a number rather than a shape. Written out rather
                    // than drawn into the path: text in the drawing would be scaled with it, and
                    // an axis that changes size with its plot is an axis nobody can read.
                    column(class = "zgui-devtools__axis") {
                        text() {{format!("{:.0} ms", BUDGET * CEILING / 1000.0)}}
                        text(class = "zgui-devtools__axis-budget") {
                            {format!("{:.1} ms", BUDGET / 1000.0)}
                        }
                        text() {"0"}
                    }
                    // No view box. A view box is fitted *uniformly*, so a plot written in one
                    // aspect ratio and given a box of another draws at the smaller of the two
                    // scales and sits centred in the leftover — which is a graph occupying a
                    // fraction of the panel it was given. Without one the path is in the element's
                    // own pixels, which is what a chart wants: the numbers that decide where the
                    // line goes are the same numbers that decided where the box went.
                    vector(
                        class = "zgui-devtools__graph",
                        node_ref = plot,
                        prop:d = move || PropValue::from(
                            trace(history, extent.get(), plot.scale()).as_str()
                        )
                    )
                }
                row(class = "zgui-devtools__row") {
                    text(class = "zgui-devtools__key") {"worst of them"}
                    text(
                        class = "zgui-devtools__value",
                        class:zgui-devtools__value-slow = move || worst(history) > BUDGET
                    ) {
                        {move || format!("{:.1} ms", worst(history) / 1000.0)}
                    }
                }
                text(class = "zgui-devtools__note") {
                    {move || format!(
                        "The rule across the middle is {:.1} ms, one refresh at 60 Hz. Anything \
                         above it is a frame that took longer than the refresh it was drawn for.",
                        BUDGET / 1000.0
                    )}
                }
            }
            row(class = "zgui-devtools__strip") {
                for slice in move || slices(timeline), key = {|slice: &Slice| slice.at} {
                    box(
                        class = "zgui-devtools__slice",
                        class = {format!("zgui-devtools__slice-{}", slice.category.suffix())},
                        class:zgui-devtools__slice-slow = {slice.slow},
                        style:width = {Some(slice.width.clone())}
                    )
                }
            }
            // Static: seven chips that say what the colours mean. Nothing here reads a signal, so
            // the legend is built once with the tab and never asks for a frame of its own.
            row(class = "zgui-devtools__legend") {
                for which in || Category::ALL, key = |which: &Category| *which {
                    row(class = "zgui-devtools__chip") {
                        box(
                            class = "zgui-devtools__dot",
                            class = {format!("zgui-devtools__dot-{}", which.suffix())}
                        )
                        text() {{which.label()}}
                    }
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
                    // One line per stage: the words, the mark's own name in brackets after them,
                    // and the cost. The raw name is kept because it is what somebody about to read
                    // the source searches for — but on its own row it doubled the height of the
                    // one list in the panel that is already too long to see at once.
                    row(class = "zgui-devtools__row") {
                        box(
                            class = "zgui-devtools__dot",
                            class = {format!("zgui-devtools__dot-{}", slice.category.suffix())}
                        )
                        text(class = "zgui-devtools__stage") {{slice.label.clone()}}
                        Show(when = {let raw = slice.raw.clone(); move || !raw.is_empty()}) {
                            text(class = "zgui-devtools__stage-mark") {
                                {format!("({})", slice.raw)}
                            }
                        }
                        text(
                            class = "zgui-devtools__cost",
                            class:zgui-devtools__value-slow = {slice.slow}
                        ) {
                            {format!("{:.1} us", slice.stage.us)}
                        }
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
        .map(|(at, stage)| {
            let (label, category) = describe(&stage.name);
            Slice {
                at,
                label: label.unwrap_or(&stage.name).to_owned(),
                raw: if label.is_some() {
                    stage.name.clone()
                } else {
                    String::new()
                },
                category,
                width: width(stage.us, total),
                slow: stage.us > total * SLOW,
                stage,
            }
        })
        .collect()
}

/// The recorded frames as a line, plus the budget drawn under it as a rule.
///
/// Two subpaths in one drawing: the trace, and a horizontal line at the refresh budget so a reader
/// can see at a glance which side of it the frames are on.
///
/// The samples are reduced to [`PLOT_W`] columns by taking the **worst** frame in each column
/// rather than the average. A graph of frame times exists to show the stutter, and a mean over
/// thirty samples is exactly the operation that hides one.
fn trace(
    history: RwSignal<Vec<f64>, LocalStorage>,
    extent: Option<zgui::geom::Rect<zgui::geom::DevicePx, zgui::geom::Device>>,
    scale: f32,
) -> String {
    let samples = history.get();
    let Some(extent) = extent else {
        // Before the first layout there is no box to draw in, and a path in a space of no size is
        // a line along the top edge rather than nothing — so draw nothing until there is one.
        return String::new();
    };
    let width = f64::from(extent.size.width.0 / scale);
    let height = f64::from(extent.size.height.0 / scale);
    if samples.is_empty() || width <= 0.0 || height <= 0.0 {
        return String::new();
    }

    // The budget rule first, so the trace is drawn over it.
    let ruled = height - height / CEILING;
    let mut path = format!("M0 {ruled:.1}L{width:.1} {ruled:.1}");

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a pixel width is small and positive"
    )]
    let columns = (width as usize).max(1).min(samples.len());
    let per = (samples.len() as f64 / columns as f64).max(1.0);
    for column in 0..columns {
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a column index and a sample count are both far inside either type"
        )]
        let worst = {
            let from = (column as f64 * per) as usize;
            let upto = (((column + 1) as f64 * per) as usize).min(samples.len());
            samples[from..upto.max(from + 1)]
                .iter()
                .copied()
                .fold(0.0_f64, f64::max)
        };
        // Zero is the bottom of the plot and the ceiling is the top, so the taller the frame the
        // higher the point — which is upside down in a coordinate space that grows downwards.
        let plotted = (worst / (BUDGET * CEILING)).clamp(0.0, 1.0) * height;
        let y = height - plotted;
        #[expect(
            clippy::cast_precision_loss,
            reason = "a column index and a column count are far inside f64"
        )]
        let x = column as f64 / columns.max(1) as f64 * width;
        if column == 0 {
            path.push_str(&format!("M{x:.1} {y:.1}"));
        } else {
            path.push_str(&format!("L{x:.1} {y:.1}"));
        }
    }
    path
}

/// The longest of the recorded frames, in microseconds.
fn worst(history: RwSignal<Vec<f64>, LocalStorage>) -> f64 {
    history.get().into_iter().fold(0.0, f64::max)
}

/// How wide one stage's slice is, as a percentage of the strip.
///
/// A floor of a tenth of a percent, because a stage that took no measurable time is still a stage
/// that ran, and a slice of zero width is a stage the strip claims did not happen.
fn width(us: f64, total: f64) -> String {
    let share = if total > 0.0 { us / total } else { 0.0 };
    format!("{:.2}%", (share * 100.0).max(0.1))
}

#[cfg(test)]
mod tests {
    use zgui::geom::{Device, DevicePx, Point, Rect, Size};
    use zgui::reactive::RwSignal;

    use super::{BUDGET, trace};

    /// A box of `width` by `height` device pixels.
    fn extent(width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    /// The trace reaches the right-hand edge of the box it is drawn in.
    ///
    /// The bug this replaces: the path was written in a fixed 240-unit space and fitted through a
    /// view box, which scales *uniformly* — so in a box of any other aspect ratio the whole graph
    /// was drawn at the smaller scale and centred, taking a fraction of the width it was given.
    #[test]
    fn the_trace_spans_the_box_it_is_given() {
        let history = RwSignal::new_local(vec![1000.0_f64; 200]);
        let path = trace(history, Some(extent(466.0, 96.0)), 1.0);

        assert!(!path.is_empty(), "a graph of 200 frames drew nothing");
        // Every x in the path, which is every number following a command letter.
        let far = path
            .split(['M', 'L'])
            .filter_map(|pair| pair.split_whitespace().next())
            .filter_map(|x| x.parse::<f64>().ok())
            .fold(0.0_f64, f64::max);
        assert!(
            far > 400.0,
            "the trace stops at {far} in a 466px box, so it is drawn into a fraction of it"
        );
        assert!(far <= 466.0, "the trace runs past the box at {far}");
    }

    /// A frame past the ceiling is clamped to the top rather than drawn off it.
    #[test]
    fn a_frame_past_the_ceiling_is_clamped_to_the_top() {
        let history = RwSignal::new_local(vec![BUDGET * 40.0]);
        let path = trace(history, Some(extent(100.0, 96.0)), 1.0);

        let highest = path
            .split(['M', 'L'])
            .filter_map(|pair| pair.split_whitespace().nth(1))
            .filter_map(|y| y.parse::<f64>().ok())
            .fold(f64::MAX, f64::min);
        assert!(
            highest >= 0.0,
            "a frame forty refreshes long was drawn at {highest}, above the top of the box"
        );
    }

    /// With no box yet there is nothing to draw, rather than a line along the top edge.
    #[test]
    fn nothing_is_drawn_before_the_box_is_known() {
        let history = RwSignal::new_local(vec![1000.0_f64; 10]);
        assert_eq!(trace(history, None, 1.0), "");
        assert_eq!(trace(history, Some(extent(0.0, 0.0)), 1.0), "");
    }
}
