//! One day of a calendar's grid.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect, RwSignal};
use zgui::vocab::UiState;
use zgui::{component, view};

use crate::calendar::date::Date;
use crate::calendar::locale::Locale;
use crate::calendar::range::DateRange;

/// What every day of one calendar shares.
///
/// Published by the calendar and read by each day, so a cell needs four props rather than a dozen
/// and the calendar's key handling has one place to write.
#[derive(Copy, Clone)]
pub struct DayContext {
    /// Which day has the calendar's single tab stop.
    ///
    /// The roving tab stop rather than a copy of `:focus`: it says which of the forty-two days is
    /// the one <kbd>Tab</kbd> reaches, and it has an answer while the calendar holds no focus at
    /// all. Whether a day is *focused* is the engine's to say, and a sheet asks it with
    /// `:focus-visible`.
    ///
    /// A date is plain data, so this is an ordinary signal rather than a local one — which is what
    /// lets the month grid derived from it be a memo, and therefore what stops every arrow key
    /// rebuilding forty-two cells that have not changed.
    pub active: RwSignal<Date>,
    /// Which day the calendar is asking to be given real focus, when it is asking.
    ///
    /// Set by the key handler and by the month buttons; cleared by the day that takes it. A
    /// separate signal from [`DayContext::active`] because a calendar that is merely *shown*
    /// must not pull focus off whatever the reader was doing.
    pub wanted: RwSignal<Option<Date>, LocalStorage>,
    /// Which day is chosen, when one is.
    pub selected: Signal<Option<Date>, LocalStorage>,
    /// Which span is chosen, when the calendar is choosing spans.
    ///
    /// Separate from [`DayContext::selected`] rather than a widening of it, because a span half
    /// chosen has a first day and no last one — a state a pair of dates cannot hold — and because a
    /// calendar choosing one day has no span to report at all.
    pub range: Signal<Option<DateRange>, LocalStorage>,
    /// Whether the whole calendar is out of action.
    pub disabled: Signal<bool, LocalStorage>,
}

impl DayContext {
    /// The context an enclosing calendar published.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }
}

/// One day cell.
///
/// A control rather than a box: a day is pressed, and a reader meets it as a grid cell that can be
/// chosen. Only the active day is in the tab order, which is what makes a calendar one tab stop
/// rather than forty-two.
#[component]
pub fn CalendarDay(
    /// Which day this is.
    date: Date,
    /// Whether it belongs to the month being shown, as opposed to the one on either side.
    in_month: bool,
    /// Whether it is today.
    today: bool,
    /// Whether this particular day cannot be chosen.
    #[prop(default = false)]
    unavailable: bool,
    /// Told when the day is pressed.
    on_choose: zgui::reactive::UnsyncCallback<Date>,
    /// Classes merged after the day's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    let context = DayContext::current().expect("a day is written inside a calendar");
    let locale = Locale::current();
    let element = NodeRef::new();

    // Chosen either way: one day when the calendar chooses days, and any day of the span when it
    // chooses spans. A reader is told the same thing in both, because being in the chosen span is
    // what "selected" means to one.
    let place = move || context.range.get().map(|span| span.place(date));
    let is_selected = move || {
        context.selected.get() == Some(date)
            || place().is_some_and(|place| place != crate::calendar::range::RangePlace::Outside)
    };
    let is_active = move || context.active.get() == date;
    let is_disabled = move || unavailable || context.disabled.get();

    // Real focus, moved only when the calendar asked for it. Mounted per cell rather than as one
    // effect over the grid because the cell that has to take focus may not exist yet when the ask
    // is made: stepping a month builds forty-two new cells, and the right one focuses itself as it
    // arrives.
    let moving = RenderEffect::new(move |_| {
        // The handle binds as the element is built, and this effect's first run is the component's
        // body — which is *before* that. Reading it here is what brings the effect back when the
        // cell exists: a month stepped into asks for the focus while its cells are still being
        // made, and focusing a handle that is bound to nothing focuses nothing and says so to
        // nobody.
        let bound = element.get().is_some();
        if bound && context.wanted.get() == Some(date) {
            element.focus();
            context.wanted.set(None);
        }
    });
    on_cleanup_local(move || drop(moving));

    let semantics = A11yBinding::new(Role::GridCell)
        .label(locale.day_label(date))
        .selected(is_selected)
        .disabled(is_disabled);

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-calendar__day"), true)
        .attribute(zgui::view::AttrName::new("data-date"), Some(date.to_iso()))
        .attribute(
            zgui::view::AttrName::new("data-outside"),
            Some((!in_month).to_string()),
        )
        .attribute(
            zgui::view::AttrName::new("data-today"),
            Some(today.to_string()),
        )
        .attribute(zgui::view::AttrName::new("data-selected"), move || {
            Some(is_selected().to_string())
        })
        // Which of the two chosen shapes this day wears: the pill of a single choice, or one end of
        // a span. A day in the middle of a span is chosen and wears neither.
        .attribute(
            zgui::view::AttrName::new("data-selected-single"),
            move || Some((context.selected.get() == Some(date) && place().is_none()).to_string()),
        )
        .attribute(zgui::view::AttrName::new("data-range"), move || {
            place().and_then(|place| place.name()).map(str::to_owned)
        })
        .state(UiState::DISABLED, is_disabled)
        .a11y_from(semantics);

    view! {
        control(
            node_ref = element,
            // One tab stop for the whole grid: every other day is reachable by arrow key and
            // reachable programmatically, and none of them is reachable by tabbing.
            tabindex = move || if is_active() { Focus::Sequential } else { Focus::Programmatic },
            on:click = move |_| {
                if !is_disabled() {
                    context.active.set(date);
                    on_choose.run(date);
                }
            },
            on:focus_in = move |_| context.active.set(date),
            {..own},
            {..attrs},
            class = class
        ) {
            box(class = "zui-calendar__day-mark") {{date.day().to_string()}}
        }
    }
}
