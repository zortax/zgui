//! What the renderer is holding on the device, and what each of the window's caches holds.
//!
//! Two halves, and the second is the one with a policy behind it.
//!
//! The renderer's own five numbers are split the way the renderer splits them, because the split is
//! the diagnosis: scratch that grows is a window that got bigger, atlases that grow are glyphs or
//! images nothing is evicting, and buffers that grow are a display list that stopped being reused.
//!
//! Under them is every cache the window budgets, each against the level it states. That is where
//! "atlases that grow" stops being a symptom and becomes an answer — a cache over its level is a
//! cache eviction is not keeping up with, and a cache that states no level says so rather than
//! showing a blank.

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
            Line(name = "fixed", value = move || bytes(frame.get().memory.fixed))
            Line(name = "targets", value = move || bytes(frame.get().memory.targets))
            Line(name = "scratch", value = move || bytes(frame.get().memory.scratch))
            Line(name = "atlases", value = move || bytes(frame.get().memory.atlases))
            Line(name = "buffers", value = move || bytes(frame.get().memory.buffers))
            Line(name = "total", value = move || bytes(frame.get().memory.total()))
            text(class = "zgui-devtools__note") {
                "A renderer that draws nothing reports nothing: these read zero under a stub, and \
                 under this machine's own device they are what it has allocated."
            }
            text(class = "zgui-devtools__head") {"what each cache holds, against its level"}
            for line in move || frame.get().budget.clone(), key = |line: &Held| line.id {
                Line(name = line.id.name(), value = move || held(&line))
            }
            text(class = "zgui-devtools__note") {
                "A cache with no level states none on purpose: nothing it holds can be produced \
                 again, or something below it already bounds what it holds."
            }
        }
    }
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
