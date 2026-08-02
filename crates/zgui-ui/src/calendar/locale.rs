//! What a month and a day are called, and which day a week starts on.

use zgui::prelude::*;

use crate::calendar::date::{Date, Weekday};

/// The names and conventions a calendar is written in.
///
/// Published as a context, so one declaration at the root of an application reaches every calendar
/// and every date field below it, and a component of one's own reaches the same names:
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::calendar::{Locale, Weekday};
///
/// /// A page whose calendars start their weeks on Monday.
/// #[component]
/// fn Page() -> impl IntoView {
///     Locale::provide(Locale {
///         first_day: Weekday::Monday,
///         ..Locale::english()
///     });
///     view! { box() }
/// }
/// ```
///
/// # There is no locale database behind this
///
/// The defaults are English, the collation is ASCII case folding, and nothing here consults CLDR,
/// the operating system or an environment variable. That is a deliberate floor rather than an
/// oversight: a component library that pulled in a locale database would pull in several megabytes
/// of it, and an application that needs Arabic month names, a non-Gregorian calendar or
/// locale-aware collation supplies them here — which is exactly the shape this type has.
///
/// What that costs, stated plainly: an application that does nothing gets English names and
/// Gregorian months, and no amount of system configuration changes that.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Locale {
    /// Which day a week is drawn as starting on.
    pub first_day: Weekday,
    /// The months, from January.
    pub months: [&'static str; 12],
    /// The days, from Monday, in full.
    pub weekdays: [&'static str; 7],
    /// The days, from Monday, short enough for a column heading.
    pub weekdays_short: [&'static str; 7],
}

impl Locale {
    /// English names, with the week starting on Sunday.
    #[must_use]
    pub const fn english() -> Self {
        Self {
            first_day: Weekday::Sunday,
            months: [
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ],
            weekdays: [
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
                "Sunday",
            ],
            weekdays_short: ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"],
        }
    }

    /// Publishes this locale to every scope below the current one.
    pub fn provide(locale: Self) {
        provide_local_context(locale);
    }

    /// The locale the nearest enclosing scope published, or the English defaults.
    ///
    /// Never `None`: a calendar with no names is a calendar of empty cells, so the floor is a real
    /// locale rather than an absence a caller has to handle.
    #[must_use]
    pub fn current() -> Self {
        use_local_context::<Self>().unwrap_or_else(Self::english)
    }

    /// What a month is called.
    #[must_use]
    pub fn month_name(&self, month: u8) -> &'static str {
        self.months[(month.clamp(1, 12) - 1) as usize]
    }

    /// What a day of the week is called.
    #[must_use]
    pub const fn weekday_name(&self, day: Weekday) -> &'static str {
        self.weekdays[day.index()]
    }

    /// The short form of a day's name, for a column heading.
    #[must_use]
    pub const fn weekday_short(&self, day: Weekday) -> &'static str {
        self.weekdays_short[day.index()]
    }

    /// The seven days in the order this locale draws them.
    #[must_use]
    pub fn week(&self) -> [Weekday; 7] {
        let mut days = [self.first_day; 7];
        for (step, day) in days.iter_mut().enumerate() {
            *day = self.first_day.advance(step);
        }
        days
    }

    /// A month and a year, as a calendar's heading.
    #[must_use]
    pub fn month_heading(&self, date: Date) -> String {
        format!("{} {}", self.month_name(date.month()), date.year())
    }

    /// A whole date, in words, which is what a reader is told a day cell is.
    #[must_use]
    pub fn day_label(&self, date: Date) -> String {
        format!(
            "{} {} {} {}",
            self.weekday_name(date.weekday()),
            date.day(),
            self.month_name(date.month()),
            date.year()
        )
    }
}

impl Default for Locale {
    fn default() -> Self {
        Self::english()
    }
}

#[cfg(test)]
mod tests {
    use super::Locale;
    use crate::calendar::date::{Date, Weekday};

    #[test]
    fn a_week_is_drawn_from_the_day_the_locale_starts_it_on() {
        let sunday = Locale::english();
        assert_eq!(sunday.week()[0], Weekday::Sunday);
        assert_eq!(sunday.week()[6], Weekday::Saturday);

        let monday = Locale {
            first_day: Weekday::Monday,
            ..Locale::english()
        };
        assert_eq!(monday.week()[0], Weekday::Monday);
        assert_eq!(monday.week()[6], Weekday::Sunday);
    }

    #[test]
    fn a_day_is_named_in_full_so_a_reader_never_meets_a_bare_number() {
        let locale = Locale::english();
        let day = Date::new(2026, 7, 24).expect("a real date");
        assert_eq!(locale.day_label(day), "Friday 24 July 2026");
        assert_eq!(locale.month_heading(day), "July 2026");
    }

    #[test]
    fn a_calendar_with_nobody_above_it_still_has_names() {
        zgui::reactive::install().ok();
        let scope = zgui::reactive::Mounted::new();
        let names = scope.with(Locale::current);
        assert_eq!(names.month_name(1), "January");
        scope.unmount();
    }

    #[test]
    fn a_published_locale_reaches_the_scopes_below_it() {
        zgui::reactive::install().ok();
        let outer = zgui::reactive::Mounted::new();
        outer.with(|| {
            Locale::provide(Locale {
                months: [
                    "Janvier",
                    "Février",
                    "Mars",
                    "Avril",
                    "Mai",
                    "Juin",
                    "Juillet",
                    "Août",
                    "Septembre",
                    "Octobre",
                    "Novembre",
                    "Décembre",
                ],
                first_day: Weekday::Monday,
                ..Locale::english()
            });
        });
        let inner = outer.with(zgui::reactive::Mounted::new);
        let names = inner.with(Locale::current);
        assert_eq!(names.month_name(7), "Juillet");
        assert_eq!(names.week()[0], Weekday::Monday);
        outer.unmount();
    }
}
