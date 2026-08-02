//! The six-by-seven block of days a month is drawn as.

use crate::calendar::date::{Date, Weekday};

/// How many weeks a month grid always has.
///
/// Six, always, rather than however many the month needs. A grid that changed height between March
/// and April would move everything below it twice a year, and a keyboard walk that ran off the end
/// of a five-week month would land somewhere different depending on which month it started in.
pub const WEEKS: usize = 6;

/// How many days a week has.
pub const DAYS: usize = 7;

/// How many cells a month grid holds.
pub const CELLS: usize = WEEKS * DAYS;

/// One month, as the days a calendar draws.
///
/// The block always starts on the locale's first day of the week and always holds six weeks, so the
/// first few and last few cells belong to the months on either side. Those are real dates rather
/// than blanks, because a calendar that blanks them is one where the arrow keys stop working at the
/// edges of a month.
///
/// ```
/// use zgui_ui::calendar::{Date, MonthGrid, Weekday};
///
/// let july = MonthGrid::of(Date::new(2026, 7, 24).expect("a real date"), Weekday::Sunday);
/// assert_eq!(july.days().len(), 42);
/// assert_eq!(july.days()[0].weekday(), Weekday::Sunday);
/// // The 1st of July 2026 is a Wednesday, so three days of June come first.
/// assert_eq!(july.days()[3], Date::new(2026, 7, 1).expect("a real date"));
/// assert_eq!(july.index_of(Date::new(2026, 7, 1).expect("a real date")), Some(3));
/// ```
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct MonthGrid {
    /// The month the grid is of, as its first day.
    month: Date,
    /// The first cell, which is on or before the first of the month.
    start: Date,
}

impl MonthGrid {
    /// The grid holding `date`'s month, with weeks starting on `first_day`.
    #[must_use]
    pub const fn of(date: Date, first_day: Weekday) -> Self {
        let month = date.first_of_month();
        let lead = month.weekday().days_since(first_day) as i32;
        Self {
            month,
            start: month.add_days(-lead),
        }
    }

    /// The first day of the month this grid is of.
    #[must_use]
    pub const fn month(self) -> Date {
        self.month
    }

    /// The first cell of the grid, which may be in the month before.
    #[must_use]
    pub const fn start(self) -> Date {
        self.start
    }

    /// The date in cell `index`, counting across then down.
    #[must_use]
    pub const fn at(self, index: usize) -> Date {
        self.start.add_days(index as i32)
    }

    /// Which cell `date` is in, when it is in this grid at all.
    #[must_use]
    pub fn index_of(self, date: Date) -> Option<usize> {
        let offset = date.to_days() - self.start.to_days();
        (0..CELLS as i64)
            .contains(&offset)
            .then_some(offset as usize)
    }

    /// Every cell, in order.
    #[must_use]
    pub fn days(self) -> Vec<Date> {
        (0..CELLS).map(|index| self.at(index)).collect()
    }

    /// The days of one week of the grid, counting from zero.
    #[must_use]
    pub fn week(self, week: usize) -> Vec<Date> {
        (0..DAYS).map(|day| self.at(week * DAYS + day)).collect()
    }

    /// Whether the date in cell `index` belongs to the month this grid is of.
    ///
    /// What decides whether a cell is drawn faded: the days on either side are shown, and are
    /// reachable, but they are not this month's.
    #[must_use]
    pub const fn is_in_month(self, index: usize) -> bool {
        self.at(index).same_month(self.month)
    }
}

#[cfg(test)]
mod tests {
    use super::{CELLS, DAYS, MonthGrid, WEEKS};
    use crate::calendar::date::{Date, Weekday};

    #[test]
    fn every_month_of_a_decade_is_six_weeks_starting_on_the_locales_first_day() {
        for first_day in [Weekday::Sunday, Weekday::Monday, Weekday::Saturday] {
            for year in 2020..2030 {
                for month in 1..=12 {
                    let date = Date::new(year, month, 1).expect("a real date");
                    let grid = MonthGrid::of(date, first_day);
                    assert_eq!(grid.days().len(), CELLS);
                    assert_eq!(grid.days()[0].weekday(), first_day);
                    assert!(
                        grid.days()[0] <= date,
                        "the grid started after the month it is of",
                    );
                    assert!(
                        grid.index_of(date).is_some_and(|index| index < DAYS),
                        "the first of the month must be in the first week",
                    );
                }
            }
        }
    }

    #[test]
    fn the_whole_month_is_in_the_grid_and_nothing_is_repeated() {
        let grid = MonthGrid::of(
            Date::new(2026, 2, 10).expect("a real date"),
            Weekday::Monday,
        );
        let days = grid.days();
        for day in 1..=28 {
            let date = Date::new(2026, 2, day).expect("a real date");
            assert!(days.contains(&date), "{date} is missing from its own grid");
        }
        let mut sorted = days.clone();
        sorted.dedup();
        assert_eq!(sorted.len(), days.len(), "a day was drawn twice");
    }

    #[test]
    fn the_cells_on_either_side_are_real_days_of_the_neighbouring_months() {
        let grid = MonthGrid::of(Date::new(2026, 7, 1).expect("a real date"), Weekday::Sunday);
        assert!(!grid.is_in_month(0), "the first cell is June's");
        assert_eq!(grid.at(0), Date::new(2026, 6, 28).expect("a real date"));
        assert!(!grid.is_in_month(CELLS - 1), "the last cell is August's");
        assert!(grid.is_in_month(3));
    }

    #[test]
    fn a_week_of_the_grid_is_seven_consecutive_days() {
        let grid = MonthGrid::of(Date::new(2026, 7, 1).expect("a real date"), Weekday::Sunday);
        for week in 0..WEEKS {
            let days = grid.week(week);
            assert_eq!(days.len(), DAYS);
            for pair in days.windows(2) {
                assert_eq!(pair[1], pair[0].add_days(1));
            }
        }
    }

    #[test]
    fn a_date_outside_the_grid_is_not_in_it() {
        let grid = MonthGrid::of(Date::new(2026, 7, 1).expect("a real date"), Weekday::Sunday);
        assert_eq!(grid.index_of(Date::new(2027, 1, 1).expect("real")), None);
        assert_eq!(grid.index_of(Date::new(2020, 1, 1).expect("real")), None);
    }
}
