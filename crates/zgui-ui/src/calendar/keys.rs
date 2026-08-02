//! Which key moves a calendar's focus where.

use zgui::vocab::{Key, Modifiers, NamedKey};

use crate::calendar::date::{Date, Weekday};

/// Where a key sends a calendar's focus.
///
/// A separate value from the date it lands on, so the mapping can be tested without a calendar and
/// so a caller that wants the same keys over its own surface can reuse it.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Move {
    /// Forward or back by whole days.
    Days(i32),
    /// Forward or back by whole months, with the day clamped.
    Months(i32),
    /// Forward or back by whole years, with the day clamped.
    Years(i32),
    /// To the first day of the week the focus is in.
    WeekStart,
    /// To the last day of that week.
    WeekEnd,
}

impl Move {
    /// Where `from` ends up, in a calendar whose weeks start on `first_day`.
    #[must_use]
    pub const fn apply(self, from: Date, first_day: Weekday) -> Date {
        match self {
            Self::Days(days) => from.add_days(days),
            Self::Months(months) => from.add_months(months),
            Self::Years(years) => from.add_years(years),
            Self::WeekStart => from.add_days(-(from.weekday().days_since(first_day) as i32)),
            Self::WeekEnd => from.add_days(6 - from.weekday().days_since(first_day) as i32),
        }
    }
}

/// What a key means to a calendar grid, or `None` when it means nothing there.
///
/// The set the authoring practices call for, and nothing beyond it: the arrows move a day and a
/// week, the page keys move a month and — with <kbd>Shift</kbd> — a year, and <kbd>Home</kbd> and
/// <kbd>End</kbd> go to the ends of the week. Everything else is left alone, so <kbd>Tab</kbd> still
/// leaves the calendar and <kbd>Escape</kbd> still reaches the surface that opened it.
///
/// ```
/// use zgui::vocab::{Key, Modifiers, NamedKey};
/// use zgui_ui::calendar::{Date, Move, Weekday, key_move};
///
/// let monday = Date::new(2026, 7, 20).expect("a real date");
/// let down = key_move(&Key::Named(NamedKey::ArrowDown), Modifiers::NONE);
/// assert_eq!(down, Some(Move::Days(7)));
/// assert_eq!(
///     down.expect("a move").apply(monday, Weekday::Monday),
///     Date::new(2026, 7, 27).expect("a real date"),
/// );
/// assert_eq!(key_move(&Key::Named(NamedKey::Escape), Modifiers::NONE), None);
/// ```
#[must_use]
pub fn key_move(key: &Key, modifiers: Modifiers) -> Option<Move> {
    let Key::Named(named) = key else {
        return None;
    };
    let shifted = modifiers.contains(Modifiers::SHIFT);
    Some(match named {
        NamedKey::ArrowLeft => Move::Days(-1),
        NamedKey::ArrowRight => Move::Days(1),
        NamedKey::ArrowUp => Move::Days(-7),
        NamedKey::ArrowDown => Move::Days(7),
        NamedKey::Home => Move::WeekStart,
        NamedKey::End => Move::WeekEnd,
        NamedKey::PageUp if shifted => Move::Years(-1),
        NamedKey::PageDown if shifted => Move::Years(1),
        NamedKey::PageUp => Move::Months(-1),
        NamedKey::PageDown => Move::Months(1),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{Move, key_move};
    use crate::calendar::date::{Date, Weekday};
    use zgui::vocab::{Key, Modifiers, NamedKey};

    /// The move a named key makes, with no modifier held.
    fn plain(key: NamedKey) -> Option<Move> {
        key_move(&Key::Named(key), Modifiers::NONE)
    }

    #[test]
    fn the_arrows_move_a_day_and_a_week_in_each_direction() {
        assert_eq!(plain(NamedKey::ArrowLeft), Some(Move::Days(-1)));
        assert_eq!(plain(NamedKey::ArrowRight), Some(Move::Days(1)));
        assert_eq!(plain(NamedKey::ArrowUp), Some(Move::Days(-7)));
        assert_eq!(plain(NamedKey::ArrowDown), Some(Move::Days(7)));
    }

    #[test]
    fn shift_turns_the_page_keys_from_months_into_years() {
        assert_eq!(plain(NamedKey::PageDown), Some(Move::Months(1)));
        assert_eq!(
            key_move(&Key::Named(NamedKey::PageDown), Modifiers::SHIFT),
            Some(Move::Years(1)),
        );
    }

    #[test]
    fn home_and_end_land_on_the_locales_own_week_boundaries() {
        let wednesday = Date::new(2026, 7, 22).expect("a real date");
        assert_eq!(
            Move::WeekStart.apply(wednesday, Weekday::Sunday),
            Date::new(2026, 7, 19).expect("a real date"),
        );
        assert_eq!(
            Move::WeekStart.apply(wednesday, Weekday::Monday),
            Date::new(2026, 7, 20).expect("a real date"),
            "the same key lands somewhere else in a locale whose week starts on Monday",
        );
        assert_eq!(
            Move::WeekEnd.apply(wednesday, Weekday::Monday),
            Date::new(2026, 7, 26).expect("a real date"),
        );
    }

    #[test]
    fn a_key_the_calendar_does_not_use_is_left_for_something_else() {
        for key in [
            NamedKey::Tab,
            NamedKey::Escape,
            NamedKey::Enter,
            NamedKey::Space,
        ] {
            assert_eq!(plain(key), None, "{key:?} was claimed by the calendar");
        }
        assert_eq!(
            key_move(
                &Key::Character(zgui::vocab::SharedString::from("j")),
                Modifiers::NONE
            ),
            None,
        );
    }

    #[test]
    fn a_month_step_from_the_end_of_a_month_lands_on_a_day_that_exists() {
        let end = Date::new(2026, 3, 31).expect("a real date");
        assert_eq!(
            Move::Months(-1).apply(end, Weekday::Monday),
            Date::new(2026, 2, 28).expect("a real date"),
        );
    }
}
