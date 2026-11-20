//! CSS, animation, clipping and responsive layout.
use crate::lab::State;
use zgui::prelude::*;
use zgui::reactive::RenderEffect;
use zgui_ui::prelude::*;

/// Starts the gravity sensor while tilt is on and this tab is shown. The first reading after
/// switching on is the zero point, so the tile starts centred however the phone is held.
/// Readings already posted can arrive after this tab closes, so they write only app-level state.
fn follow_tilt(s: State) {
    let zero = s.tilt_zero;
    let sensor = RenderEffect::new(move |_| {
        let on = s.tilting.get();
        zero.set(None);
        s.tilt.set((0.0, 0.0));
        #[cfg(not(target_os = "android"))]
        let _ = on;
        #[cfg(target_os = "android")]
        zgui::app::android::on_tilt(on.then(|| {
            let ui = ui();
            Box::new(move |x, y| {
                ui.post(move || {
                    let (zx, zy) = zero.get_untracked().unwrap_or_else(|| {
                        zero.set(Some((x, y)));
                        (x, y)
                    });
                    s.tilt.set((x - zx, y - zy));
                })
            }) as Box<dyn Fn(f32, f32) + Send>
        }));
    });
    zgui::reactive::on_cleanup_local(move || {
        drop(sensor);
        #[cfg(target_os = "android")]
        zgui::app::android::on_tilt(None);
    });
}

#[component]
pub(crate) fn Visuals() -> impl IntoView {
    let s = State::current();
    follow_tilt(s);
    let shift = move || {
        let (x, y) = s.tilt.get();
        Some(format!("translate({}px, {}px)", x * 120.0, y * 120.0))
    };
    view! {
        column(class = "stack") {
            column(class = "card") {
                row(class = "setting") {
                    column() { text(class = "section-title") {"Motion & paint"} text(class = "muted") {"Transforms, gradients, shadows, clipping"} }
                    spacer()
                    Switch(checked = s.motion, a11y:label = "Run animation")
                }
                row(class = "setting") {
                    column() { text(class = "section-title") {"Tilt"} text(class = "muted") {"Gravity sensor moves the tile"} }
                    spacer()
                    Switch(checked = s.tilting, a11y:label = "Tilt")
                }
                column(class = "stage", class:running = move || s.motion.get()) {
                    column(class = "tilt", style:transform = shift) {
                        column(class = "orbit") { text {"Z"} }
                    }
                }
            }
            column(class = "swatches") {
                column(class = "swatch gradient") { text {"Gradient"} text(class = "swatch-value") {"4 colour stops"} }
                column(class = "swatch rounded") { text {"Rounded"} text(class = "swatch-value") {"40 px radius"} }
                column(class = "swatch shadow") { text {"Shadow"} text(class = "swatch-value") {"Two layers"} }
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
