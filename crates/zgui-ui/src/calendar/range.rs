//! Choosing a span of days rather than one.

use crate::calendar::date::Date;

/// How many days a calendar lets a reader choose.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum CalendarMode {
    /// One day, which pressing again clears.
    #[default]
    Single,
    /// A first day and a last day, chosen with two presses.
    Range,
}

impl CalendarMode {
    /// The value written to `data-mode`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Range => "range",
        }
    }
}

/// A first day and, once a second press has said so, a last one.
///
/// `to` is `None` between the two presses, which is the state a range spends half its life in and
/// the one a pair of dates cannot express. A reader who has pressed once has said where the span
/// starts and nothing else.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct DateRange {
    /// The first day of the span.
    pub from: Date,
    /// The last day, once there is one.
    pub to: Option<Date>,
}

impl DateRange {
    /// A span of one day, waiting for its second press.
    #[must_use]
    pub const fn starting(from: Date) -> Self {
        Self { from, to: None }
    }

    /// A settled span, with the two ends put in order.
    #[must_use]
    pub fn between(one: Date, other: Date) -> Self {
        let (from, to) = if other < one {
            (other, one)
        } else {
            (one, other)
        };
        Self { from, to: Some(to) }
    }

    /// Whether both ends are known.
    #[must_use]
    pub const fn is_settled(self) -> bool {
        self.to.is_some()
    }

    /// How `date` sits in the span.
    #[must_use]
    pub fn place(self, date: Date) -> RangePlace {
        let Some(to) = self.to else {
            return if date == self.from {
                RangePlace::Start
            } else {
                RangePlace::Outside
            };
        };
        if date < self.from || date > to {
            RangePlace::Outside
        } else if date == self.from {
            RangePlace::Start
        } else if date == to {
            RangePlace::End
        } else {
            RangePlace::Middle
        }
    }

    /// Whether the span covers `date`.
    #[must_use]
    pub fn contains(self, date: Date) -> bool {
        self.place(date) != RangePlace::Outside
    }
}

/// What one press does to whatever span is already chosen.
///
/// A settled span is replaced rather than stretched: once both ends are known, a reader pressing a
/// third day has told the calendar where their new span begins, and a calendar that instead moved
/// whichever end happened to be nearer would be one where the same press means two things.
#[must_use]
pub fn extend(current: Option<DateRange>, date: Date) -> DateRange {
    match current {
        Some(range) if !range.is_settled() => DateRange::between(range.from, date),
        _ => DateRange::starting(date),
    }
}

/// Where one day sits in a chosen span.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum RangePlace {
    /// Not in the span at all.
    Outside,
    /// The first day.
    Start,
    /// Between the ends.
    Middle,
    /// The last day.
    End,
}

impl RangePlace {
    /// The value written to `data-range`, when there is one to write.
    #[must_use]
    pub const fn name(self) -> Option<&'static str> {
        match self {
            Self::Outside => None,
            Self::Start => Some("start"),
            Self::Middle => Some("middle"),
            Self::End => Some("end"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CalendarMode, DateRange, RangePlace, extend};
    use crate::calendar::date::Date;

    /// A date that exists, by day of July 2026.
    fn july(day: u8) -> Date {
        Date::new(2026, 7, day).expect("a real date")
    }

    #[test]
    fn a_second_press_before_the_first_puts_the_ends_the_right_way_round() {
        let started = extend(None, july(20));
        assert_eq!(started, DateRange::starting(july(20)));

        let settled = extend(Some(started), july(14));
        assert_eq!(settled.from, july(14));
        assert_eq!(settled.to, Some(july(20)));
    }

    #[test]
    fn a_third_press_begins_a_new_span_rather_than_stretching_the_old_one() {
        let settled = DateRange::between(july(10), july(14));
        let next = extend(Some(settled), july(20));
        assert_eq!(next, DateRange::starting(july(20)));
    }

    #[test]
    fn every_day_of_a_span_knows_which_end_of_it_it_is() {
        let span = DateRange::between(july(10), july(13));
        assert_eq!(span.place(july(9)), RangePlace::Outside);
        assert_eq!(span.place(july(10)), RangePlace::Start);
        assert_eq!(span.place(july(11)), RangePlace::Middle);
        assert_eq!(span.place(july(13)), RangePlace::End);
        assert_eq!(span.place(july(14)), RangePlace::Outside);
    }

    #[test]
    fn a_half_chosen_span_marks_its_first_day_and_nothing_else() {
        let half = DateRange::starting(july(10));
        assert!(!half.is_settled());
        assert_eq!(half.place(july(10)), RangePlace::Start);
        assert_eq!(half.place(july(11)), RangePlace::Outside);
        assert_eq!(CalendarMode::Range.name(), "range");
    }
}
