//! The data a chart draws.

/// One measurement.
#[derive(Clone, PartialEq, Debug)]
pub struct Datum {
    /// What the measurement is of, which is what the category axis is labelled with.
    pub label: String,
    /// What was measured.
    pub value: f64,
}

impl Datum {
    /// A measurement of `value`, called `label`.
    #[must_use]
    pub fn new(label: impl Into<String>, value: f64) -> Self {
        Self {
            label: label.into(),
            value,
        }
    }
}

/// One named run of measurements.
#[derive(Clone, PartialEq, Debug)]
pub struct Series {
    /// What the run is called, which is what a legend and a reader are told.
    pub name: String,
    /// The measurements, in the order they are drawn.
    pub points: Vec<Datum>,
    /// Which of the theme's chart colours to draw it in, counting from one.
    ///
    /// The tokens rather than a colour, so a chart follows the theme and follows a reader's dark
    /// mode without the chart knowing either exists. Out of range wraps.
    pub tone: usize,
}

impl Series {
    /// A series called `name`, drawn in the theme's first chart colour.
    #[must_use]
    pub fn new(name: impl Into<String>, points: Vec<Datum>) -> Self {
        Self {
            name: name.into(),
            points,
            tone: 1,
        }
    }

    /// The same series in the theme's `tone`th chart colour.
    #[must_use]
    pub const fn toned(mut self, tone: usize) -> Self {
        self.tone = tone;
        self
    }

    /// Every value in the series.
    #[must_use]
    pub fn values(&self) -> Vec<f64> {
        self.points.iter().map(|point| point.value).collect()
    }

    /// Which custom property holds this series' colour.
    ///
    /// Five tones, wrapping, because that is what the token set defines — a sixth series drawn in a
    /// colour nothing declared would be an invisible series.
    #[must_use]
    pub fn colour_token(&self) -> String {
        let tone = if self.tone == 0 {
            1
        } else {
            (self.tone - 1) % 5 + 1
        };
        format!("var(--zui-color-chart-{tone})")
    }
}

/// How a series is drawn.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum ChartKind {
    /// One bar per measurement, from zero.
    #[default]
    Bar,
    /// A line through the measurements, with a mark at each.
    Line,
    /// A line, with the space between it and zero filled.
    Area,
}

impl ChartKind {
    /// The value written to `data-kind`, which is what the sheet selects on.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::Line => "line",
            Self::Area => "area",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ChartKind, Datum, Series};

    #[test]
    fn a_series_colour_wraps_round_the_five_the_theme_declares() {
        let points = vec![Datum::new("a", 1.0)];
        assert_eq!(
            Series::new("s", points.clone()).colour_token(),
            "var(--zui-color-chart-1)",
        );
        assert_eq!(
            Series::new("s", points.clone()).toned(5).colour_token(),
            "var(--zui-color-chart-5)",
        );
        assert_eq!(
            Series::new("s", points.clone()).toned(6).colour_token(),
            "var(--zui-color-chart-1)",
            "a sixth series is drawn in the first colour rather than in none",
        );
        assert_eq!(
            Series::new("s", points).toned(0).colour_token(),
            "var(--zui-color-chart-1)",
        );
    }

    #[test]
    fn the_values_of_a_series_are_its_points_in_order() {
        let series = Series::new(
            "sales",
            vec![Datum::new("jan", 3.0), Datum::new("feb", -1.0)],
        );
        assert_eq!(series.values(), vec![3.0, -1.0]);
    }

    #[test]
    fn every_kind_has_a_name_a_sheet_can_select_on() {
        for kind in [ChartKind::Bar, ChartKind::Line, ChartKind::Area] {
            assert!(!kind.name().is_empty());
        }
        assert_eq!(ChartKind::default(), ChartKind::Bar);
    }
}
