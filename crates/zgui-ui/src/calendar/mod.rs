//! A month of days, walked with the arrow keys.

mod date;
mod day;
mod grid;
mod keys;
mod locale;
mod range;
mod style;

pub use crate::calendar::date::{Date, Weekday, days_in_month, is_leap_year};
pub use crate::calendar::day::{CalendarDay, CalendarDayProps, DayContext};
pub use crate::calendar::grid::{CELLS, DAYS, MonthGrid, WEEKS};
pub use crate::calendar::keys::{Move, key_move};
pub use crate::calendar::locale::Locale;
pub use crate::calendar::range::{CalendarMode, DateRange, RangePlace, extend as extend_range};
pub use crate::calendar::style::CalendarStyle;

use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, Memo, RwSignal, UnsyncCallback};
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::{CHEVRON_LEFT, CHEVRON_RIGHT};
use zgui_ui_primitives::{Binding, Controllable};

/// What the calendar's rules are installed under.
pub(crate) const SHEET: &str = "zui-calendar";

/// Whether a date can be chosen.
///
/// A closure rather than a range, because the real cases are not ranges: no weekends, nothing
/// already booked, nothing more than sixty days out. A range is one line of closure.
pub type DateFilter = Rc<dyn Fn(Date) -> bool>;

/// A month of days, with the keyboard the authoring practices ask for.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{RwSignal, UnsyncCallback};
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// When to arrive.
/// #[component]
/// fn Arrival() -> impl IntoView {
///     let chosen = RwSignal::new_local(Date::new(2026, 7, 24));
///     view! {
///         Calendar(
///             value = chosen,
///             label = "Arrival date",
///             on_change = UnsyncCallback::new(move |date: Option<Date>| chosen.set(date))
///         )
///     }
/// }
/// ```
///
/// # One day or a span
///
/// [`CalendarMode::Single`] chooses one day, which pressing again clears. [`CalendarMode::Range`]
/// chooses a span: the first press says where it starts, the second says where it ends — either way
/// round — and a third begins the next span. A chosen span is drawn as one band with a filled pill
/// at each end, which is why the two ends and the days between them are three different states
/// rather than one.
///
/// `months` shows that many months side by side, which is what a span picked across a month
/// boundary wants. The block follows the keyboard only when the keyboard walks off it, so arrowing
/// from the first month into the second moves nothing.
///
/// # Keyboard
///
/// One tab stop for the whole grid, on whichever day the focus is on. Inside it:
///
/// | Key | What it does |
/// |---|---|
/// | <kbd>←</kbd> <kbd>→</kbd> | a day back, a day on |
/// | <kbd>↑</kbd> <kbd>↓</kbd> | a week back, a week on |
/// | <kbd>Home</kbd> <kbd>End</kbd> | the first and last day of the week |
/// | <kbd>PageUp</kbd> <kbd>PageDown</kbd> | a month back, a month on |
/// | <kbd>Shift</kbd> with the page keys | a year back, a year on |
/// | <kbd>Enter</kbd> <kbd>Space</kbd> | choose the focused day — the framework's own activation |
///
/// Moving off the edge of the month shows the next one and keeps the focus on the day it landed
/// on, so a reader arrowing forward walks from one month into the next without ever pressing a
/// button. Nothing here claims <kbd>Tab</kbd> or <kbd>Escape</kbd>: a calendar inside a popover has
/// to be leavable.
///
/// # What a reader is told
///
/// A grid, whose rows are weeks and whose cells are days. Each cell is named in full — *Friday 24
/// July 2026* — rather than by its number, because a number on its own is not a date and a reader
/// moving through a grid of numbers has to hold the month in their head. The heading is a live
/// region, so stepping a month is announced.
///
/// # There is no clock here
///
/// `today` is a prop. This library reads no clock, holds no time zone and asks the operating system
/// nothing, because "today" is a question about where the reader is rather than about where the
/// process is, and a component that guessed would be wrong on the two days a year it matters most.
#[component]
pub fn Calendar(
    /// Which day is chosen, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<Option<Date>>,
    /// Which day starts chosen, when the calendar owns it itself.
    #[prop(optional)]
    default_value: Option<Date>,
    /// Told whenever the chosen day changes.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<Option<Date>>>,
    /// Whether a press chooses one day or one end of a span.
    #[prop(default = CalendarMode::Single)]
    mode: CalendarMode,
    /// Which span is chosen, when the caller holds it.
    ///
    /// Read and written only in [`CalendarMode::Range`]; a calendar choosing single days has no
    /// span to report.
    #[prop(into, optional)]
    range: Binding<Option<DateRange>>,
    /// Which span starts chosen, when the calendar owns it itself.
    #[prop(optional)]
    default_range: Option<DateRange>,
    /// Told whenever the chosen span changes.
    #[prop(optional)]
    on_range_change: Option<UnsyncCallback<Option<DateRange>>>,
    /// How many months to show side by side.
    ///
    /// Zero is one: a calendar showing no months is not a calendar, and a caller computing this
    /// from a width has one fewer edge to think about.
    #[prop(default = 1)]
    months: usize,
    /// Which month is shown first. Defaults to the chosen day's month.
    #[prop(optional)]
    default_month: Option<Date>,
    /// Which day to mark as today, when the caller knows.
    #[prop(optional)]
    today: Option<Date>,
    /// Which days can be chosen. Every day, unless the caller says otherwise.
    #[prop(optional)]
    available: Option<DateFilter>,
    /// Whether any of it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// What the calendar is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Where to record the calendar's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the calendar's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, CalendarStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let locale = Locale::current();
    let first_day = locale.first_day;

    let shown = months.max(1);

    let chosen = Controllable::new(value, default_value, on_change);
    let span = Controllable::new(range, default_range, on_range_change);
    // Which month is shown first and where the single tab stop is are two pieces of state, because
    // a calendar showing two months has a focused day that can move between them without the block
    // moving at all. They are kept in step below: the block follows the focus only when the focus
    // has walked off it.
    //
    // Which month to open on is the caller's `default_month`, then whichever day is already
    // chosen — however it is held. Reading the controlled value here rather than only the
    // uncontrolled one is what stops `<Calendar value=some_day/>`, which is the ordinary
    // controlled form, opening on the epoch instead of on the day it is showing as chosen.
    let start = default_month
        .or(default_value)
        .or_else(|| chosen.get_untracked())
        .or_else(|| span.get_untracked().map(|span| span.from))
        .or_else(|| default_range.map(|span| span.from))
        .or(today)
        .unwrap_or_else(|| Date::from_days(0));
    let active = RwSignal::new(start);
    let anchor = RwSignal::new(start.first_of_month());
    let wanted = RwSignal::new_local(None::<Date>);

    let selected = Signal::derive_local(move || match mode {
        CalendarMode::Single => chosen.get(),
        CalendarMode::Range => None,
    });
    let chosen_span = Signal::derive_local(move || match mode {
        CalendarMode::Single => None,
        CalendarMode::Range => span.get(),
    });
    provide_local_context(day::DayContext {
        active,
        wanted,
        selected,
        range: chosen_span,
        disabled,
    });

    let can_choose: DateFilter = available.unwrap_or_else(|| Rc::new(|_| true));
    // Remembered rather than recomputed. Every arrow key moves the focused day, and all but two
    // of them land in the month already on the screen — where the grid is the *same grid*. A
    // derived value would answer that with a fresh notification each time and the forty-two cells
    // would be torn down and rebuilt under the keyboard, taking the element that was about to be
    // focused with them. A memo publishes nothing when the answer has not changed.
    let grids: Vec<Memo<MonthGrid>> = (0..shown)
        .map(|offset| {
            let step = i32::try_from(offset).unwrap_or(0);
            Memo::new(move |_| MonthGrid::of(anchor.get().add_months(step), first_day))
        })
        .collect();

    // The block follows the focus only when the focus has left it, which is what lets a two-month
    // calendar be walked from the first month into the second without the months moving.
    let keep_in_view = move |date: Date| {
        let first = anchor.get_untracked();
        let last = first.add_months(i32::try_from(shown.saturating_sub(1)).unwrap_or(0));
        if date < first || date > last.last_of_month() {
            anchor.set(date.first_of_month());
        }
    };

    let step_month = move |by: i32| {
        let next = active.get_untracked().add_months(by);
        active.set(next);
        anchor.set(anchor.get_untracked().add_months(by));
        wanted.set(Some(next));
    };

    // Pressing the chosen day again clears the choice, which is what makes a chosen date
    // un-choosable from the keyboard at all — a calendar with no way back is one where a mistake
    // is permanent. A span is never cleared by pressing: the third press starts the next one.
    let choose = UnsyncCallback::new(move |date: Date| match mode {
        CalendarMode::Single => {
            let next = if chosen.get_untracked() == Some(date) {
                None
            } else {
                Some(date)
            };
            chosen.set(next);
        }
        CalendarMode::Range => {
            span.set(Some(range::extend(span.get_untracked(), date)));
        }
    });

    let mut semantics = A11yBinding::new(Role::Group);
    if let Some(text) = label.clone() {
        semantics = semantics.label(text);
    }
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-calendar"), true)
        .class_toggle(zgui::view::ClassName::new(CalendarStyle::CLASS), true)
        .a11y_from(semantics);

    let grid_label = label.unwrap_or_else(|| String::from("Calendar"));
    let can_choose = zgui::reactive::StoredValue::new_local(can_choose);

    let blocks: Vec<AnyView> = grids
        .into_iter()
        .enumerate()
        .map(|(offset, grid)| {
            let heading_locale = locale.clone();
            let heading =
                Signal::derive_local(move || heading_locale.month_heading(grid.get().month()));
            // Only the first month's heading is announced. Six headings changing together is one
            // event to a reader and six interruptions to a screen reader.
            let live = if offset == 0 {
                zgui::vocab::Live::Polite
            } else {
                zgui::vocab::Live::Off
            };
            AnyView::new(view! {
                box(class = "zui-calendar__month") {
                    box(class = "zui-calendar__caption") {
                        box(class = "zui-calendar__heading", a11y:live = live) {
                            {move || heading.get()}
                        }
                    }
                    box(
                        class = "zui-calendar__grid",
                        a11y:role = Role::Grid,
                        a11y:label = grid_label.clone(),
                        on:key_down = move |ev| {
                            if disabled.get_untracked() {
                                return;
                            }
                            let Some(asked) = key_move(&ev.key, ev.modifiers) else { return };
                            let next = asked.apply(active.get_untracked(), first_day);
                            active.set(next);
                            keep_in_view(next);
                            wanted.set(Some(next));
                            // Claimed, so the same arrow does not also scroll whatever the calendar
                            // is in.
                            ev.prevent_default();
                            ev.stop_propagation();
                        }
                    ) {
                        {weekday_strip(&locale)}
                        {move || {
                            let grid = grid.get();
                            (0..WEEKS)
                                .map(|week| {
                                    let days: Vec<AnyView> = (0..DAYS)
                                        .map(|column| {
                                            let index = week * DAYS + column;
                                            let date = grid.at(index);
                                            let allowed = can_choose
                                                .with_value(|filter| filter(date));
                                            AnyView::new(view! {
                                                CalendarDay(
                                                    date = date,
                                                    in_month = grid.is_in_month(index),
                                                    today = today == Some(date),
                                                    unavailable = !allowed,
                                                    on_choose = choose
                                                )
                                            })
                                        })
                                        .collect();
                                    AnyView::new(view! {
                                        box(class = "zui-calendar__week", a11y:role = Role::Row) {
                                            {days}
                                        }
                                    })
                                })
                                .collect::<Vec<AnyView>>()
                        }}
                    }
                }
            })
        })
        .collect();

    view! {
        box(node_ref = element, {..own}, {..attrs}, class = class) {
            box(class = "zui-calendar__months") {
                row(class = "zui-calendar__nav") {
                    control(
                        class = "zui-calendar__step",
                        tabindex = Focus::Sequential,
                        a11y:label = "Previous month",
                        a11y:disabled = move || disabled.get(),
                        attr:data-disabled = move || Some(disabled.get().to_string()),
                        on:click = move |_| if !disabled.get_untracked() { step_month(-1) }
                    ) {
                        Icon(icon = CHEVRON_LEFT)
                    }
                    control(
                        class = "zui-calendar__step",
                        tabindex = Focus::Sequential,
                        a11y:label = "Next month",
                        a11y:disabled = move || disabled.get(),
                        attr:data-disabled = move || Some(disabled.get().to_string()),
                        on:click = move |_| if !disabled.get_untracked() { step_month(1) }
                    ) {
                        Icon(icon = CHEVRON_RIGHT)
                    }
                }
                {blocks}
            }
        }
    }
}

/// The row of weekday names over a month's columns.
fn weekday_strip(locale: &Locale) -> impl IntoView {
    let days: Vec<AnyView> = locale
        .week()
        .into_iter()
        .map(|day| {
            let short = locale.weekday_short(day);
            let full = locale.weekday_name(day);
            AnyView::new(view! {
                box(
                    class = "zui-calendar__weekday",
                    a11y:role = Role::ColumnHeader,
                    a11y:label = full
                ) {
                    {short}
                }
            })
        })
        .collect();
    view! { box(class = "zui-calendar__week", a11y:role = Role::Row) {{days}} }
}
