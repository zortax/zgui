//! Proleptic Gregorian dates, and the arithmetic a calendar walks them with.
//!
//! No dependency, no time zone and no clock: a calendar shows days, and a day is a year, a month
//! and a number. The conversion to and from a day count is Howard Hinnant's civil-calendar
//! algorithm, which is exact for every year this type admits and needs no table.

use core::cmp::Ordering;
use core::fmt;

/// A day of the week.
///
/// Numbered from Monday, because the arithmetic wants a fixed origin and the *display* order — which
/// starts on Sunday in some places and Monday in others — is [`Locale`](crate::calendar::Locale)'s
/// business rather than this type's.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Weekday {
    /// Monday.
    Monday,
    /// Tuesday.
    Tuesday,
    /// Wednesday.
    Wednesday,
    /// Thursday.
    Thursday,
    /// Friday.
    Friday,
    /// Saturday.
    Saturday,
    /// Sunday.
    Sunday,
}

impl Weekday {
    /// Every day, in order from Monday.
    pub const ALL: [Self; 7] = [
        Self::Monday,
        Self::Tuesday,
        Self::Wednesday,
        Self::Thursday,
        Self::Friday,
        Self::Saturday,
        Self::Sunday,
    ];

    /// Which day this is, counting Monday as zero.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The day `steps` days after this one, wrapping round the week.
    #[must_use]
    pub const fn advance(self, steps: usize) -> Self {
        Self::ALL[(self as usize + steps) % 7]
    }

    /// How many days it is from `first` forward to this one.
    ///
    /// What decides which column a date lands in: a week starting on Sunday puts Sunday in column
    /// zero, and a week starting on Monday puts it in column six.
    #[must_use]
    pub const fn days_since(self, first: Self) -> usize {
        (self as usize + 7 - first as usize) % 7
    }
}

/// One day of the proleptic Gregorian calendar.
///
/// A plain triple with no clock behind it: two dates that name the same day are the same date, and
/// nothing here depends on where the program is running or what time it is.
///
/// ```
/// use zgui_ui::calendar::{Date, Weekday};
///
/// let day = Date::new(2026, 7, 24).expect("a real date");
/// assert_eq!(day.weekday(), Weekday::Friday);
/// assert_eq!(day.to_iso(), "2026-07-24");
///
/// // A month is stepped, not added to blindly: the 31st of January plus a month is the 28th.
/// let end = Date::new(2026, 1, 31).expect("a real date");
/// assert_eq!(end.add_months(1), Date::new(2026, 2, 28).expect("a real date"));
///
/// // Anything that is not a day is not a date.
/// assert!(Date::new(2025, 2, 29).is_none());
/// assert!(Date::new(2026, 13, 1).is_none());
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Date {
    /// The year, which may be negative and may be zero: this is the proleptic calendar.
    year: i32,
    /// The month, from one to twelve.
    month: u8,
    /// The day of the month, from one.
    day: u8,
}

impl Date {
    /// The date, when `year`, `month` and `day` name a day that exists.
    ///
    /// `None` for the 31st of April and the 29th of February in a common year, which is the whole
    /// of the validation: a calendar that accepted them would show a cell nobody can mean.
    #[must_use]
    pub const fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        if month < 1 || month > 12 || day < 1 {
            return None;
        }
        if day > days_in_month(year, month) {
            return None;
        }
        Some(Self { year, month, day })
    }

    /// The date, with the day clamped to the last day of the month it lands in.
    ///
    /// The right answer for month arithmetic — the 31st of January stepped one month forward is the
    /// end of February — and the wrong answer for parsing, which is why it is a separate call.
    #[must_use]
    pub const fn clamped(year: i32, month: u8, day: u8) -> Self {
        let month = if month < 1 {
            1
        } else if month > 12 {
            12
        } else {
            month
        };
        let last = days_in_month(year, month);
        let day = if day < 1 {
            1
        } else if day > last {
            last
        } else {
            day
        };
        Self { year, month, day }
    }

    /// The year.
    #[must_use]
    pub const fn year(self) -> i32 {
        self.year
    }

    /// The month, from one to twelve.
    #[must_use]
    pub const fn month(self) -> u8 {
        self.month
    }

    /// The day of the month, from one.
    #[must_use]
    pub const fn day(self) -> u8 {
        self.day
    }

    /// Which day of the week this is.
    #[must_use]
    pub const fn weekday(self) -> Weekday {
        // 1970-01-01 was a Thursday, and the day count is measured from it.
        let days = self.to_days();
        let index = (days + 3).rem_euclid(7) as usize;
        Weekday::ALL[index]
    }

    /// The first day of this date's month.
    #[must_use]
    pub const fn first_of_month(self) -> Self {
        Self {
            year: self.year,
            month: self.month,
            day: 1,
        }
    }

    /// The last day of this date's month.
    #[must_use]
    pub const fn last_of_month(self) -> Self {
        Self {
            year: self.year,
            month: self.month,
            day: days_in_month(self.year, self.month),
        }
    }

    /// Whether `other` is in the same month of the same year.
    #[must_use]
    pub const fn same_month(self, other: Self) -> bool {
        self.year == other.year && self.month == other.month
    }

    /// The date `days` days after this one, or before it when `days` is negative.
    #[must_use]
    pub const fn add_days(self, days: i32) -> Self {
        Self::from_days(self.to_days() + days as i64)
    }

    /// The date `months` months after this one, with the day clamped to the month it lands in.
    #[must_use]
    pub const fn add_months(self, months: i32) -> Self {
        let total = self.year as i64 * 12 + (self.month as i64 - 1) + months as i64;
        let year = total.div_euclid(12) as i32;
        let month = total.rem_euclid(12) as u8 + 1;
        Self::clamped(year, month, self.day)
    }

    /// The date `years` years after this one, with the 29th of February clamped to the 28th.
    #[must_use]
    pub const fn add_years(self, years: i32) -> Self {
        Self::clamped(self.year + years, self.month, self.day)
    }

    /// How many days since 1970-01-01, which is negative before it.
    ///
    /// The one representation every comparison and every step goes through, so that "the day after"
    /// is addition rather than a month-length table with a leap-year branch in it.
    #[must_use]
    pub const fn to_days(self) -> i64 {
        let year = self.year as i64 - if self.month <= 2 { 1 } else { 0 };
        let era = if year >= 0 { year } else { year - 399 } / 400;
        let year_of_era = year - era * 400;
        let month = self.month as i64;
        let day_of_year =
            (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + self.day as i64 - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }

    /// The date `days` days after 1970-01-01.
    #[must_use]
    pub const fn from_days(days: i64) -> Self {
        let shifted = days + 719_468;
        let era = if shifted >= 0 {
            shifted
        } else {
            shifted - 146_096
        } / 146_097;
        let day_of_era = shifted - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let year = year_of_era + era * 400;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u8;
        let month = (shifted_month + if shifted_month < 10 { 3 } else { -9 }) as u8;
        Self {
            year: (year + if month <= 2 { 1 } else { 0 }) as i32,
            month,
            day,
        }
    }

    /// The date as `YYYY-MM-DD`, which is the one spelling that sorts as text and means one thing
    /// everywhere.
    #[must_use]
    pub fn to_iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// Reads a date written as `YYYY-MM-DD`.
    ///
    /// `None` for anything else at all, including a real date written another way: this is the
    /// interchange spelling, not a guess at what somebody meant.
    ///
    /// ```
    /// use zgui_ui::calendar::Date;
    ///
    /// assert_eq!(Date::from_iso("2026-07-24"), Date::new(2026, 7, 24));
    /// assert!(Date::from_iso("24/07/2026").is_none());
    /// assert!(Date::from_iso("2026-02-30").is_none());
    /// ```
    #[must_use]
    pub fn from_iso(text: &str) -> Option<Self> {
        let mut parts = text.split('-');
        let (year, month, day) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
            (Some(year), Some(month), Some(day), None) => (year, month, day),
            _ => return None,
        };
        if month.len() != 2 || day.len() != 2 {
            return None;
        }
        Self::new(year.parse().ok()?, month.parse().ok()?, day.parse().ok()?)
    }
}

impl PartialOrd for Date {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Date {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.year, self.month, self.day).cmp(&(other.year, other.month, other.day))
    }
}

impl fmt::Display for Date {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_iso())
    }
}

/// Whether `year` has a 29th of February.
#[must_use]
pub const fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// How many days `month` has in `year`.
///
/// Answers 31 for a month number outside one to twelve, which is the safe direction: this is only
/// reached through [`Date::clamped`], which has already put the month in range.
#[must_use]
pub const fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::{Date, Weekday, days_in_month, is_leap_year};

    #[test]
    fn a_date_survives_the_round_trip_through_a_day_count() {
        let mut day = Date::new(1969, 12, 28).expect("a real date");
        for _ in 0..4_000 {
            assert_eq!(Date::from_days(day.to_days()), day, "{day} did not survive");
            day = day.add_days(1);
        }
    }

    #[test]
    fn the_weekday_is_right_at_the_dates_everybody_knows() {
        // The epoch, a leap day, and the far side of a century that is not a leap year.
        assert_eq!(
            Date::new(1970, 1, 1).expect("a real date").weekday(),
            Weekday::Thursday
        );
        assert_eq!(
            Date::new(2000, 2, 29).expect("a real date").weekday(),
            Weekday::Tuesday
        );
        assert_eq!(
            Date::new(1900, 3, 1).expect("a real date").weekday(),
            Weekday::Thursday
        );
        assert_eq!(
            Date::new(2026, 7, 24).expect("a real date").weekday(),
            Weekday::Friday
        );
    }

    #[test]
    fn the_century_rule_is_the_one_everybody_gets_wrong() {
        assert!(is_leap_year(2000));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2100));
        assert!(is_leap_year(2024));
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
    }

    #[test]
    fn a_date_before_the_epoch_is_an_ordinary_date() {
        // Proleptic: the Gregorian rules are run backwards past the point they were adopted, so
        // this is not the weekday a chronicler of the day would have written.
        let old = Date::new(1066, 10, 14).expect("a real date");
        assert!(old.to_days() < 0);
        assert_eq!(Date::from_days(old.to_days()), old);
        assert_eq!(old.weekday(), Weekday::Sunday);
        assert_eq!(
            old.add_days(1).weekday(),
            Weekday::Monday,
            "the week runs the same way before the epoch as after it",
        );
    }

    #[test]
    fn stepping_a_month_clamps_rather_than_overflowing_into_the_next_one() {
        let end = Date::new(2026, 1, 31).expect("a real date");
        assert_eq!(end.add_months(1), Date::new(2026, 2, 28).expect("real"));
        assert_eq!(end.add_months(13), Date::new(2027, 2, 28).expect("real"));
        assert_eq!(end.add_months(-1), Date::new(2025, 12, 31).expect("real"));
        // Twelve steps of one month is not one step of twelve months when the day clamps, and
        // that is the correct behaviour rather than a defect: each step lands somewhere real.
        assert_eq!(
            Date::new(2024, 2, 29).expect("real").add_years(1),
            Date::new(2025, 2, 28).expect("real"),
        );
    }

    #[test]
    fn stepping_a_month_backwards_across_a_year_boundary_lands_in_the_right_year() {
        let january = Date::new(2026, 1, 15).expect("a real date");
        assert_eq!(
            january.add_months(-1),
            Date::new(2025, 12, 15).expect("real")
        );
        assert_eq!(
            january.add_months(-13),
            Date::new(2024, 12, 15).expect("real")
        );
    }

    #[test]
    fn the_days_between_two_weekdays_is_the_column_a_date_lands_in() {
        assert_eq!(Weekday::Sunday.days_since(Weekday::Monday), 6);
        assert_eq!(Weekday::Sunday.days_since(Weekday::Sunday), 0);
        assert_eq!(Weekday::Monday.days_since(Weekday::Sunday), 1);
    }

    #[test]
    fn a_date_that_is_not_a_day_is_refused() {
        assert!(Date::new(2025, 2, 29).is_none());
        assert!(Date::new(2024, 2, 29).is_some());
        assert!(Date::new(2026, 4, 31).is_none());
        assert!(Date::new(2026, 0, 1).is_none());
        assert!(Date::new(2026, 1, 0).is_none());
    }

    #[test]
    fn iso_text_round_trips_and_refuses_anything_else() {
        let day = Date::new(2026, 7, 24).expect("a real date");
        assert_eq!(Date::from_iso(&day.to_iso()), Some(day));
        assert_eq!(Date::from_iso("2026-7-24"), None, "the parts are padded");
        assert_eq!(Date::from_iso("2026-07-24-01"), None);
        assert_eq!(Date::from_iso(""), None);
    }

    #[test]
    fn dates_order_the_way_days_do() {
        let mut day = Date::new(2025, 11, 28).expect("a real date");
        for _ in 0..200 {
            let next = day.add_days(1);
            assert!(day < next, "{day} did not come before {next}");
            day = next;
        }
    }
}
