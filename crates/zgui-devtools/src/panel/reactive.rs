//! What the reactive side of the program is holding.
//!
//! The tab is deliberately about **scopes** rather than about signals, because scopes are what can
//! be asked. The dependency edges a graph view would need are private to `reactive_graph` — the
//! subscriber sets are `pub(crate)`, and the public traits only add to and remove from them — so
//! "which effects would re-run if this signal changed" is not a question this build can answer.
//!
//! What it can answer is the one that catches real bugs: every signal, memo and effect belongs to
//! the scope that was current when it was made, every component instance is one scope, and a view
//! that is not disposing of its scopes is a view whose live count climbs with the number it has
//! ever built.

use zgui::prelude::*;
use zgui::{component, view};

#[allow(
    unused_imports,
    reason = "the tag names the component and the macro names its props type"
)]
use crate::panel::frame::{Line, LineProps};
use crate::sample::reactive::Live;
use crate::state::DevTools;

/// The reactivity tab.
#[component]
pub(crate) fn ReactivePanel(
    /// Where what is alive is published.
    tools: DevTools,
) -> impl IntoView {
    let reactive = tools.reactive;
    view! {
        column(class = "zgui-devtools__body") {
            text(class = "zgui-devtools__head") {"what the reactive graph is holding"}
            Line(name = "instances alive", value = move || reactive.get().alive.to_string())
            Line(name = "instances built", value = move || reactive.get().built.to_string())
            Line(name = "deepest scope", value = move || reactive.get().deepest.to_string())
            text(class = "zgui-devtools__note") {
                "Every signal, memo and effect belongs to the scope that was current when it was \
                 made, and every component instance is one scope. A view that reuses its instances \
                 keeps the first number flat while the second climbs; one that leaks them climbs \
                 both together."
            }
            text(class = "zgui-devtools__head") {"live instances, by component"}
            Show(
                when = move || !reactive.get().components.is_empty(),
                fallback = move || view! {
                    text(class = "zgui-devtools__value-quiet") {
                        {move || if reactive.get().instrumented {
                            "Nothing is mounted."
                        } else {
                            "This build records no component boundaries, so there is nothing to \
                             count."
                        }}
                    }
                }
            ) {
                for live in move || reactive.get().components.clone(),
                    key = |live: &Live| live.name.clone()
                {
                    row(class = "zgui-devtools__row") {
                        text(class = "zgui-devtools__stage") {{live.name.clone()}}
                        text(class = "zgui-devtools__stage-mark") {
                            {format!("({}:{})", trim(live.source.0), live.source.1)}
                        }
                        text(class = "zgui-devtools__cost") {
                            {if live.least == live.most {
                                format!("{} at depth {}", live.alive, live.least)
                            } else {
                                format!("{} at depth {}-{}", live.alive, live.least, live.most)
                            }}
                        }
                    }
                }
            }
        }
    }
}

/// A path cut down to the file at the end of it.
fn trim(file: &str) -> &str {
    file.rsplit('/').next().unwrap_or(file)
}
