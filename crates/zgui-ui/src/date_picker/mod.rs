//! A button that opens a calendar.

mod style;

pub use crate::date_picker::style::DatePickerStyle;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::vocab::{HasPopup, UiState};
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_DOWN;
use zgui_ui_primitives::{Binding, Controllable};

use crate::calendar::{CalendarMode, CalendarProps, Date, DateFilter, DateRange, Locale};
use crate::overlay::{AnchoredSurfaceProps, OverlayState};

/// What the date picker's rules are installed under.
const SHEET: &str = "zui-date-picker";

/// A field that shows a date and opens a calendar to change it.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// When the invoice is due.
/// #[component]
/// fn Due() -> impl IntoView {
///     let due = RwSignal::new_local(None::<Date>);
///     view! { DatePicker(value = due, label = "Due date", placeholder = "Pick a date") }
/// }
/// ```
///
/// # Keyboard
///
/// The trigger is a button, so <kbd>Enter</kbd> and <kbd>Space</kbd> open it. Inside, the calendar
/// has the whole of the authoring practices' date grid — see [`Calendar`](crate::Calendar). <kbd>Escape</kbd> closes
/// the surface and gives focus back to the trigger, and <kbd>Tab</kbd> is confined to the surface
/// while it is open, so the calendar cannot be tabbed past without being dealt with.
///
/// Choosing a day closes the picker. A date picker that stayed open after a choice would be one
/// where every reader presses Escape out of habit and wonders whether the choice took.
///
/// # What a reader is told
///
/// A button that opens a dialog, naming the chosen date in words — *Friday 24 July 2026* — or the
/// placeholder when nothing is chosen. The surface is a dialog rather than a plain container, so an
/// assistive technology announces arriving in it.
#[component]
pub fn DatePicker(
    /// Which day is chosen, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<Option<Date>>,
    /// Which day starts chosen, when the picker owns it itself.
    #[prop(optional)]
    default_value: Option<Date>,
    /// Told whenever the chosen day changes.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<Option<Date>>>,
    /// Whether a press chooses one day or one end of a span.
    #[prop(default = CalendarMode::Single)]
    mode: CalendarMode,
    /// Which span is chosen, when the caller holds it.
    #[prop(into, optional)]
    range: Binding<Option<DateRange>>,
    /// Which span starts chosen, when the picker owns it itself.
    #[prop(optional)]
    default_range: Option<DateRange>,
    /// Told whenever the chosen span changes.
    #[prop(optional)]
    on_range_change: Option<UnsyncCallback<Option<DateRange>>>,
    /// How many months the calendar shows side by side.
    #[prop(default = 1)]
    months: usize,
    /// Whether the surface is open, when the caller holds it too.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    open: Binding<bool>,
    /// Told whenever the surface opens or closes.
    #[prop(optional)]
    on_open_change: Option<UnsyncCallback<bool>>,
    /// Which day to mark as today, when the caller knows.
    #[prop(optional)]
    today: Option<Date>,
    /// Which days can be chosen.
    #[prop(optional)]
    available: Option<DateFilter>,
    /// What to show while nothing is chosen.
    #[prop(into, default = String::from("Pick a date"))]
    placeholder: String,
    /// Whether it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// What the field is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// The element whose text names this one.
    #[prop(optional)]
    labelled_by: Option<NodeRef>,
    /// Where to record the trigger, for relating it to something else.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the picker's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the trigger.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, DatePickerStyle::CSS);
    let locale = Locale::current();
    let chosen = Controllable::new(value, default_value, on_change);
    let span = Controllable::new(range, default_range, on_range_change);
    let state = OverlayState::new(open, false, on_open_change).provide();
    let trigger = node_ref.unwrap_or_else(|| state.trigger());

    // What is chosen, either way — one day or two — so that the trigger, the reader's value and the
    // emptiness the placeholder answers to are all one question asked once.
    let told = {
        let locale = locale.clone();
        Signal::derive_local(move || match mode {
            CalendarMode::Single => chosen.get().map(|date| locale.day_label(date)),
            CalendarMode::Range => span.get().map(|span| match span.to {
                Some(end) => format!(
                    "{} – {}",
                    locale.day_label(span.from),
                    locale.day_label(end)
                ),
                None => locale.day_label(span.from),
            }),
        })
    };
    let shown = {
        let empty = placeholder.clone();
        Signal::derive_local(move || told.get().unwrap_or_else(|| empty.clone()))
    };

    let mut semantics = A11yBinding::new(Role::Button)
        .value(move || zgui::vocab::SharedString::from(shown.get()))
        .disabled(move || disabled.get());
    if let Some(text) = label.clone() {
        semantics = semantics.label(text);
    }
    if let Some(target) = labelled_by {
        semantics = semantics.labelled_by(target);
    }

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-date-picker__trigger"), true)
        .attribute(zgui::view::AttrName::new("data-empty"), move || {
            Some(told.get().is_none().to_string())
        })
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(semantics);

    // Choosing closes: the calendar reports every change, and the picker's whole purpose is one
    // choice. Clearing a date — pressing the chosen day again — closes it too, because the reader
    // has said what they meant either way.
    let took = move |date: Option<Date>| {
        chosen.set(date);
        state.close();
    };
    // A span closes the picker only once it has both ends: closing on the first press would be a
    // picker that asks a reader to open it again to finish the answer they were half-way through.
    let took_span = move |next: Option<DateRange>| {
        span.set(next);
        if next.is_some_and(super::calendar::DateRange::is_settled) {
            state.close();
        }
    };

    let surface_label = label
        .clone()
        .unwrap_or_else(|| String::from("Choose a date"));
    // Held rather than moved: the surface is rebuilt every time it re-opens, and a filter a
    // closure moved out of on the first open is a filter the second open does not have.
    let available = zgui::reactive::StoredValue::new_local(available);
    // The calendar shows what the picker holds and every choice goes back through the picker,
    // which is what closes the surface as well as moving the value.
    let calendar_value = Binding::controlled(Signal::derive_local(move || chosen.get()), took);
    let calendar_range = Binding::controlled(Signal::derive_local(move || span.get()), took_span);

    view! {
        box(class = DatePickerStyle::CLASS, class = "zui-date-picker", class = class) {
            control(
                node_ref = trigger,
                tabindex = Focus::Sequential,
                on:click = move |_| {
                    if !disabled.get_untracked() {
                        state.toggle();
                    }
                },
                {..state.trigger_attrs(HasPopup::Dialog)},
                {..own},
                {..attrs}
            ) {
                text(class = "zui-date-picker__label") {{move || shown.get()}}
                Icon(icon = CHEVRON_DOWN)
            }
            AnchoredSurface(
                state = state,
                role = Role::Dialog,
                class = "zui-date-picker__surface",
                trap = {zgui::view::FocusTrapOptions::default()},
                a11y:label = surface_label.clone()
            ) {
                Calendar(
                    value = calendar_value,
                    mode = mode,
                    range = calendar_range,
                    months = months,
                    default_month = {default_value.or(default_range.map(|span| span.from))},
                    today = {today},
                    available = {available.get_value()},
                    label = surface_label.clone()
                )
            }
        }
    }
}
