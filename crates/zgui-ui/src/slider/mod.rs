//! A number chosen by dragging or by arrowing.

mod keys;
mod style;

pub use crate::slider::keys::{Move, key_move};
pub use crate::slider::style::SliderStyle;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_primitives::{Binding, Controllable};

use crate::support::{Bound, clamp_to_step};

/// What the slider's rules are installed under.
const SHEET: &str = "zui-slider";

/// A slider.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// How loud.
/// #[component]
/// fn Volume() -> impl IntoView {
///     let volume = RwSignal::new_local(40.0);
///     view! { Slider(value = volume, min = 0.0, max = 100.0, step = 5.0, label = "Volume") }
/// }
/// ```
///
/// # Keyboard
///
/// The authoring practices' set, and nothing beyond it: the arrow keys move one step, page up and
/// page down move ten, and home and end go to the ends. Every other key is left alone, so tab
/// still leaves the slider and a shortcut aimed at the surface still reaches it — see
/// [`key_move`].
///
/// # Dragging
///
/// A press anywhere on the track moves the value to that point and captures the pointer, so the
/// drag continues even when it leaves the slider's box. What the pointer is over stops mattering
/// the moment the press lands, which is what makes a thumb draggable at all.
///
/// # What a reader is told
///
/// The number, the range and the step, which together are what makes a slider operable without
/// looking at it. `label` names what the number measures.
#[component]
pub fn Slider(
    /// The value, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<f64>,
    /// Where it starts, when the slider owns it itself.
    #[prop(default = 0.0)]
    default_value: f64,
    /// Told whenever it changes, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<f64>>,
    /// The smallest value.
    #[prop(default = 0.0)]
    min: f64,
    /// The largest.
    #[prop(default = 100.0)]
    max: f64,
    /// How far one keystroke moves it.
    #[prop(default = 1.0)]
    step: f64,
    /// Whether it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// What the number measures, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// The element whose text names this one.
    #[prop(optional)]
    labelled_by: Option<NodeRef>,
    /// Where to record this component's own element, for relating it to something else.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the slider's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, SliderStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let bound = Bound::new(min, max, step);
    let held = Controllable::new(value, clamp_to_step(default_value, bound), on_change);
    let track = NodeRef::new();

    let mut semantics = A11yBinding::new(Role::Slider)
        .numeric_value(move || held.get())
        .disabled(move || disabled.get())
        .step(move |a11y| {
            a11y.numeric_range(bound.min, bound.max)
                .numeric_step(bound.step)
        });
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    if let Some(target) = labelled_by {
        semantics = semantics.labelled_by(target);
    }

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-slider"), true)
        .state(UiState::DISABLED, move || disabled.get())
        .custom_property(
            zgui::view::CustomPropertyName::new("zui-slider-fraction"),
            move || Some(format!("{:.4}%", bound.fraction(held.get()) * 100.0)),
        )
        .a11y_from(semantics);

    // Two conversions, and leaving either out looks like a working slider on the machine it was
    // written on. A pointer reports where it is in CSS pixels and an element in device pixels, so
    // the scale relates them; and a pointer reports where it is in the *window*, so the track has
    // to be asked where it is in the window too. Its parent-relative box would put every slider's
    // origin near zero and send the thumb to the end of the track on any press.
    let dragging = zgui::reactive::RwSignal::new_local(false);
    let to_point = move |x_css: f32| {
        let Some(box_) = track.window_bounds() else {
            return;
        };
        if box_.size.width.0 <= 0.0 {
            return;
        }
        let x = x_css * track.scale();
        let fraction = f64::from((x - box_.origin.x.0) / box_.size.width.0);
        held.set(bound.at(fraction));
    };

    view! {
        control(
            node_ref = element,
            class = SliderStyle::CLASS,
            tabindex = {Focus::Sequential},
            on:key_down = move |ev| {
                if disabled.get_untracked() {
                    return;
                }
                let Some(asked) = key_move(&ev.key, bound) else { return };
                let next = match asked {
                    Move::By(by) => held.get_untracked() + by,
                    Move::To(to) => to,
                };
                held.set(clamp_to_step(next, bound));
                // Claimed, so the framework does not also scroll the surface with the same key.
                ev.prevent_default();
                ev.stop_propagation();
            },
            on:pointer_down = move |ev| {
                if disabled.get_untracked() {
                    return;
                }
                ev.capture_pointer();
                dragging.set(true);
                to_point(ev.position.x.0);
            },
            on:pointer_move = move |ev| {
                if dragging.get_untracked() && !disabled.get_untracked() {
                    to_point(ev.position.x.0);
                }
            },
            on:pointer_up = move |ev| {
                dragging.set(false);
                ev.release_pointer();
            },
            on:pointer_cancel = move |ev| {
                dragging.set(false);
                ev.release_pointer();
            },
            {..own},
            {..attrs},
            class = class
        ) {
            box(class = "zui-slider__track", node_ref = track) {
                box(class = "zui-slider__range")
            }
            box(class = "zui-slider__thumb")
        }
    }
}
