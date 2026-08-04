//! What the renderer is holding on the device, what the document costs on the host, and what each
//! of the window's caches holds.
//!
//! Three halves, and the last two are the ones with a policy behind them.
//!
//! The renderer's own five numbers are split the way the renderer splits them, because the split is
//! the diagnosis: scratch that grows is a window that got bigger, atlases that grow are glyphs or
//! images nothing is evicting, and buffers that grow are a display list that stopped being reused.
//! They are drawn as one bar before they are listed as five numbers, because the question is which
//! of the five is the large one and a column of byte counts makes that arithmetic rather than a
//! glance.
//!
//! The document is beside them because it is the half the renderer cannot see. A window whose
//! device memory is flat while its document keeps growing has a leak in the view layer, and a memory
//! tab that only showed the device would report that as "nothing is wrong".
//!
//! Under them is every cache the window budgets, each against the level it states, and each with a
//! bar showing how much of that level is gone. That is where "atlases that grow" stops being a
//! symptom and becomes an answer — a cache over its level is a cache eviction is not keeping up
//! with, and a cache that states no level says so rather than showing a blank.

use zgui::prelude::*;
use zgui::{component, view};

#[allow(
    unused_imports,
    reason = "the macro names the props type, the tag names the component"
)]
use crate::panel::frame::{Line, LineProps};
use zgui::runtime::budget::CacheUnit;

use crate::sample::bytes;

use crate::sample::frame::Held;
use crate::state::DevTools;

/// One component of the renderer's report: what it is called and how to read it out.
type Component = (&'static str, fn(&zgui::render::MemoryReport) -> u64);

/// The renderer's report, in the order the bar stacks them.
const COMPONENTS: [Component; 5] = [
    ("fixed", |it| it.fixed),
    ("targets", |it| it.targets),
    ("scratch", |it| it.scratch),
    ("atlases", |it| it.atlases),
    ("buffers", |it| it.buffers),
];

/// The memory tab.
#[component]
pub(crate) fn MemoryPanel(
    /// Where the frame, and with it the renderer's report, is published.
    tools: DevTools,
) -> impl IntoView {
    let frame = tools.frame;
    view! {
        column(class = "zgui-devtools__body") {
            text(class = "zgui-devtools__head") {"what the renderer holds"}
            // Only when there is something to draw: a stub renderer reports zero for all five, and
            // a bar of five zero-width segments reads as a bug rather than as an empty device.
            Show(when = move || frame.get().memory.total() > 0) {
                row(class = "zgui-devtools__meter") {
                    for part in move || segments(frame), key = |part: &Segment| part.name {
                        box(
                            class = "zgui-devtools__seg",
                            class = {format!("zgui-devtools__seg-{}", part.name)},
                            style:width = {Some(part.width.clone())}
                        )
                    }
                }
            }
            for part in move || segments(frame), key = |part: &Segment| part.name {
                row(class = "zgui-devtools__row") {
                    box(
                        class = "zgui-devtools__dot",
                        class = {format!("zgui-devtools__seg-{}", part.name)}
                    )
                    text(class = "zgui-devtools__key") {{part.name}}
                    text(class = "zgui-devtools__value") {{bytes(part.held)}}
                }
            }
            Line(name = "total", value = move || bytes(frame.get().memory.total()))
            text(class = "zgui-devtools__note") {
                "A renderer that draws nothing reports nothing: these read zero under a stub, and \
                 under this machine's own device they are what it has allocated."
            }
            text(class = "zgui-devtools__head") {"what the document costs on the host"}
            Line(name = "nodes", value = move || frame.get().nodes.to_string())
            Line(name = "document", value = move || bytes(frame.get().document))
            text(class = "zgui-devtools__note") {
                "Records, arena slots, key tables and columns — the fixed cost of having a node at \
                 all. Text and attribute values hold heap of their own that this does not count."
            }
            text(class = "zgui-devtools__head") {"what each cache holds, against its level"}
            for line in move || frame.get().budget.clone(), key = |line: &Held| line.id {
                Line(name = line.id.name(), value = {let line = line.clone(); move || held(&line)})
                // Only a cache that states a level has a bar: a bar against no level would have to
                // invent a full, and an invented full is the one thing a budget panel must not do.
                Show(when = {let line = line.clone(); move || line.limit.is_some()}) {
                    row(class = "zgui-devtools__track") {
                        box(
                            class = "zgui-devtools__fill zgui-devtools__fill-pinned",
                            style:width = {Some(share(line.pinned, line.limit))}
                        )
                        box(
                            class = "zgui-devtools__fill",
                            class:zgui-devtools__fill-over = {line.over() > 0},
                            style:width = {
                                Some(share(line.resident.saturating_sub(line.pinned), line.limit))
                            }
                        )
                    }
                }
            }
            text(class = "zgui-devtools__note") {
                "A cache with no level states none on purpose: nothing it holds can be produced \
                 again, or something below it already bounds what it holds."
            }
        }
    }
}

/// One component of the renderer's report, with its share of the bar worked out.
#[derive(Clone, Debug)]
struct Segment {
    /// What it is called, which is also what colours it and what keys it.
    name: &'static str,
    /// How much it holds.
    held: u64,
    /// How wide its part of the bar is.
    width: String,
}

/// The renderer's five numbers, each as a share of their total.
///
/// Once per rebuild rather than once per segment, for the reason the timeline strip works the same
/// way: the share is a fraction of a total that has to be summed out of a signal.
fn segments(frame: zgui::reactive::RwSignal<crate::sample::Frame, LocalStorage>) -> Vec<Segment> {
    let report = frame.get().memory;
    let total = report.total();
    COMPONENTS
        .iter()
        .map(|(name, read)| {
            let held = read(&report);
            Segment {
                name,
                held,
                width: share(held, Some(total)),
            }
        })
        .collect()
}

/// What share of `whole` `part` is, as a percentage a style can take.
///
/// Capped at a hundred, because the two segments of a cache's bar are drawn side by side and a
/// cache over its level would otherwise push the second one out of the track — the overflow is
/// already said in words and in the colour, and a bar that runs off the end says nothing extra.
fn share(part: u64, whole: Option<u64>) -> String {
    #[expect(
        clippy::cast_precision_loss,
        reason = "byte counts this large are not what the difference of one part in a bar is"
    )]
    let fraction = match whole {
        Some(whole) if whole > 0 => (part as f64 / whole as f64).clamp(0.0, 1.0),
        _ => 0.0,
    };
    format!("{:.2}%", fraction * 100.0)
}

/// One cache line: what is held, what of that is pinned, and the level it is held to.
fn held(line: &Held) -> String {
    let amount = |count: u64| match line.unit {
        CacheUnit::Bytes => bytes(count),
        CacheUnit::Entries => count.to_string(),
    };
    let level = match line.limit {
        None => "no level".to_owned(),
        Some(limit) => amount(limit),
    };
    let over = if line.over() > 0 {
        format!(" — over by {}", amount(line.over()))
    } else {
        String::new()
    };
    format!(
        "{} of {level}, {} pinned{over}",
        amount(line.resident),
        amount(line.pinned)
    )
}

#[cfg(test)]
mod tests {
    use super::share;

    #[test]
    fn a_part_is_its_share_of_the_whole() {
        assert_eq!(share(1, Some(4)), "25.00%");
        assert_eq!(share(0, Some(4)), "0.00%");
        assert_eq!(share(4, Some(4)), "100.00%");
    }

    #[test]
    fn a_cache_over_its_level_still_fits_the_track() {
        // The cap is what keeps the two segments of one bar inside it. Without it a cache at twice
        // its level draws a second segment past the end of the track, which reads as a rendering
        // fault rather than as the overflow it is.
        assert_eq!(share(9, Some(4)), "100.00%");
    }

    #[test]
    fn nothing_is_a_share_of_nothing() {
        // A stub renderer reports zero for everything, and a division by that total is the one way
        // this could put a NaN into a stylesheet.
        assert_eq!(share(0, Some(0)), "0.00%");
        assert_eq!(share(5, None), "0.00%");
    }
}
