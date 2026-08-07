//! What the last frame did: what it drew, what it damaged, and every counter that moved.
//!
//! The counters are the part worth reading first. A frame's *time* is a property of the machine; a
//! frame's counters are a property of what the framework decided to redo, and every regression this
//! inspector exists to diagnose shows up here before it shows up in a duration.

use zgui::prelude::*;
use zgui::{component, view};

use crate::state::DevTools;

/// The frame tab.
#[component]
pub(crate) fn FramePanel(
    /// Where the frame is published.
    tools: DevTools,
) -> impl IntoView {
    let frame = tools.frame;
    view! {
        column(class = "zgui-devtools__body") {
            text(class = "zgui-devtools__head") {"the frame just painted"}
            Line(name = "primitives", value = move || frame.get().primitives.to_string())
            Line(name = "batches", value = move || frame.get().batches.to_string())
            Line(name = "vector passes", value = move || frame.get().passes.to_string())
            Line(
                name = "vector backend",
                value = move || crate::panel::memory::vector_status(frame.get().vector)
            )
            Line(
                name = "damage",
                value = move || {
                    let it = frame.get();
                    if it.full_damage {
                        "the whole surface".to_owned()
                    } else {
                        format!("{} rectangles", it.damage.len())
                    }
                }
            )
            Line(
                name = "of the surface",
                value = move || format!("{:.1}%", frame.get().damage_fraction() * 100.0)
            )
            // Keyed by position in the list and not by the rectangle: two damaged regions of
            // the same size at the same place are a thing a frame may genuinely report, and a
            // keyed list whose keys collide renders the second of them as nothing.
            for entry in move || {
                frame.get().damage.into_iter().take(12).enumerate().collect::<Vec<_>>()
            }, key = {|(at, _): &(usize, zgui::geom::Rect<i32, zgui::geom::Device>)| *at} {
                row(class = "zgui-devtools__row") {
                    text(class = "zgui-devtools__key") {""}
                    text(class = "zgui-devtools__value-quiet") {
                        {format!(
                            "{} x {} at {}, {}",
                            entry.1.size.width,
                            entry.1.size.height,
                            entry.1.origin.x,
                            entry.1.origin.y,
                        )}
                    }
                }
            }
            text(class = "zgui-devtools__head") {"what this frame redid"}
            Show(
                when = move || !frame.get().counters.is_empty(),
                fallback = || view! {
                    text(class = "zgui-devtools__value-quiet") {
                        "no counter moved: this frame repeated the last one"
                    }
                }
            ) {
                for counter in move || frame.get().counters,
                    key = |(name, _): &(&'static str, u64)| *name
                {
                    row(class = "zgui-devtools__row") {
                        text(class = "zgui-devtools__key") {{counter.0}}
                        text(class = "zgui-devtools__value") {{counter.1.to_string()}}
                    }
                }
            }
        }
    }
}

/// One labelled line, which every panel here is made of.
#[component]
pub(crate) fn Line<F>(
    /// What the line is called.
    name: &'static str,
    /// What it says, read afresh whenever what it reads from changes.
    value: F,
) -> impl IntoView
where
    F: Fn() -> String + 'static,
{
    view! {
        row(class = "zgui-devtools__row") {
            text(class = "zgui-devtools__key") {{name}}
            text(class = "zgui-devtools__value") {{value}}
        }
    }
}
