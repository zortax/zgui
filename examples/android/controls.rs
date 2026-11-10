//! Editing, pointer drag, binding and modal overlays.
use crate::lab::State;
use zgui::prelude::*;
use zgui_ui::prelude::*;

#[component]
pub(crate) fn Controls() -> impl IntoView {
    let s = State::current();
    let progress = Signal::derive_local(move || Some(s.volume.get()));
    view! {
        column(class = "stack") {
            column(class = "card hero") {
                text(class = "eyebrow") {"LIVE BINDINGS"}
                text(class = "section-title") {{move || format!("Hello, {}", s.name.get())}}
                text(class = "muted") {"Edit, drag and toggle. Values survive backgrounding."}
            }
            column(class = "card") {
                text(class = "section-title") {"Text input"}
                Input(value = s.name, label = "Display name", placeholder = "Your name", class = "touch-input")
                text(class = "muted") {{move || format!("{} characters", s.name.get().chars().count())}}
            }
            column(class = "card") {
                row(class = "setting") {
                    column() { text {"Notifications"} text(class = "muted") {{move || if s.enabled.get() { "Enabled" } else { "Paused" }}} }
                    spacer()
                    Switch(checked = s.enabled, a11y:label = "Notifications")
                }
                row(class = "setting") {
                    Checkbox(checked = s.checked, a11y:label = "Enable diagnostics")
                    text {"Enable diagnostics"}
                }
                text {{move || format!("Volume: {:.0}%", s.volume.get())}}
                Slider(value = s.volume, step = 1.0, label = "Volume", class = "touch-slider")
                Progress(value = progress, label = "Volume preview")
            }
            row(class = "actions") {
                Button(on:click = move |_| s.taps.update(|v| *v += 1)) {
                    {move || format!("Tapped {}", s.taps.get())}
                }
                Button(disabled = true, variant = ButtonVariant::Outline) {"Disabled"}
                Dialog(open = s.dialog) {
                    DialogTrigger(variant = ButtonVariant::Outline) {"Dialog"}
                    DialogContent(class = "mobile-dialog") {
                        DialogHeader {
                            DialogTitle {"Overlay check"}
                            DialogDescription {"Modal layering, focus containment and dismissal."}
                        }
                        text {{move || format!("Hello, {}. Volume is {:.0}%.", s.name.get(), s.volume.get())}}
                        DialogFooter { DialogClose {"Done"} }
                    }
                }
            }
        }
    }
}
