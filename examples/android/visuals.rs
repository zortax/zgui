//! CSS, animation, clipping and responsive layout.
use crate::lab::State;
use zgui::prelude::*;
use zgui_ui::prelude::*;

#[component]
pub(crate) fn Visuals() -> impl IntoView {
    let s = State::current();
    view! {
        column(class = "stack") {
            column(class = "card") {
                row(class = "setting") {
                    column() { text(class = "section-title") {"Motion & paint"} text(class = "muted") {"Transforms, gradients, shadows, clipping"} }
                    spacer()
                    Switch(checked = s.motion, a11y:label = "Run animation")
                }
                column(class = "stage", class:running = move || s.motion.get()) {
                    column(class = "orbit") { text {"Z"} }
                }
            }
            column(class = "swatches") {
                column(class = "swatch cyan") { text {"Gradient"} text(class = "swatch-value") {"01"} }
                column(class = "swatch violet") { text {"Rounded"} text(class = "swatch-value") {"02"} }
                column(class = "swatch amber") { text {"Shadow"} text(class = "swatch-value") {"03"} }
            }
            column(class = "card") {
                Accordion {
                    AccordionItem(value = "coverage") {
                        AccordionTrigger {"What does this exercise?"}
                        AccordionContent {text {"Reactive bindings, keyed reconciliation, editing, pointer capture, overlays, CSS cascade, GPU paint and Android lifecycle."}}
                    }
                    AccordionItem(value = "next") {
                        AccordionTrigger {"Try this on your phone"}
                        AccordionContent {text {"Drag volume, type a name, add 50 tasks, reverse rows, open a dialog, change theme, rotate, then background and return."}}
                    }
                }
            }
        }
    }
}
