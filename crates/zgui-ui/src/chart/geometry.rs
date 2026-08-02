//! The paths a chart's marks and axes are drawn as.
//!
//! Path notation rather than a shape type, because that is what a `<vector>` takes: one string per
//! outline, in the element's own coordinate space. Everything here is a pure function of numbers,
//! so a mark's geometry is testable without a window.

use crate::chart::scale::Scale;

/// Where a chart's plot area is inside its box, in CSS pixels.
///
/// The margins are what the axis labels are drawn in. They are part of the geometry rather than of
/// the style sheet because the marks have to be placed inside what is left, and a margin only CSS
/// knew about would put every bar in the wrong place.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Plot {
    /// The whole chart's width.
    pub width: f64,
    /// Its height.
    pub height: f64,
    /// Room for the value axis' labels, on the left.
    pub left: f64,
    /// Room above the plot, so the topmost label is not clipped.
    pub top: f64,
    /// Room for the category axis' labels, at the bottom.
    pub bottom: f64,
    /// Room to the right of the plot.
    pub right: f64,
}

impl Plot {
    /// A plot area inside a chart `width` by `height`, with the usual room for labels.
    #[must_use]
    pub const fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            left: 44.0,
            top: 8.0,
            bottom: 24.0,
            right: 8.0,
        }
    }

    /// The left edge of the plot.
    #[must_use]
    pub const fn x0(&self) -> f64 {
        self.left
    }

    /// The right edge.
    #[must_use]
    pub fn x1(&self) -> f64 {
        (self.width - self.right).max(self.left)
    }

    /// The top edge.
    #[must_use]
    pub const fn y0(&self) -> f64 {
        self.top
    }

    /// The bottom edge.
    #[must_use]
    pub fn y1(&self) -> f64 {
        (self.height - self.bottom).max(self.top)
    }

    /// How wide the plot is.
    #[must_use]
    pub fn inner_width(&self) -> f64 {
        self.x1() - self.x0()
    }

    /// How tall it is.
    #[must_use]
    pub fn inner_height(&self) -> f64 {
        self.y1() - self.y0()
    }

    /// The scale that maps values onto the plot's vertical extent, largest at the top.
    #[must_use]
    pub fn value_scale(&self, values: impl IntoIterator<Item = f64>) -> Scale {
        Scale::over(values, self.y1(), self.y0())
    }

    /// Where the `index`th of `count` evenly spaced bands starts and how wide it is.
    #[must_use]
    pub fn band(&self, index: usize, count: usize) -> (f64, f64) {
        let count = count.max(1);
        let width = self.inner_width() / count as f64;
        (self.x0() + index as f64 * width, width)
    }
}

/// A rectangle, as path notation.
#[must_use]
pub fn rect_path(x: f64, y: f64, width: f64, height: f64) -> String {
    let width = width.max(0.0);
    let height = height.max(0.0);
    format!(
        "M{x:.2} {y:.2} L{:.2} {y:.2} L{:.2} {:.2} L{x:.2} {:.2} Z",
        x + width,
        x + width,
        y + height,
        y + height,
    )
}

/// Where one mark sits in the plot, and how big it is.
///
/// A mark is its own element, so it is its own box: a pointer is over *this* bar rather than over
/// the chart, and `:hover` picks out one measurement. Marks that all shared the plot's box would
/// all be stacked on top of each other, and only whichever one was drawn last would ever be
/// hovered, hit-tested or outlined.
///
/// The rectangle is in the plot's coordinates — that is where to *put* the element — while
/// [`MarkBox::path`] is written in the element's own, which is the space a `<vector>`'s outline is
/// read in.
///
/// ```
/// use zgui_ui::chart::{MarkBox, Plot};
///
/// let plot = Plot::new(300.0, 150.0);
/// let scale = plot.value_scale([0.0, 100.0]);
/// let first = MarkBox::bar(plot.x0(), 20.0, 100.0, &scale);
/// let second = MarkBox::bar(plot.x0() + 40.0, 20.0, 50.0, &scale);
/// assert_ne!(first.x, second.x, "two bars are two places, so two boxes");
/// assert!(first.height > second.height, "and the taller value is the taller box");
/// assert!(first.path().starts_with("M0.00 0.00"), "drawn in its own box");
/// ```
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct MarkBox {
    /// The left edge, in the plot's coordinates.
    pub x: f64,
    /// The top edge, in the plot's coordinates.
    pub y: f64,
    /// How wide the mark is.
    pub width: f64,
    /// How tall it is.
    pub height: f64,
}

impl MarkBox {
    /// One bar of a bar chart: from the value axis' zero to the value.
    ///
    /// A negative value draws downwards from zero rather than upwards from the axis, which is what
    /// makes a chart with negatives readable at all.
    #[must_use]
    pub fn bar(x: f64, width: f64, value: f64, scale: &Scale) -> Self {
        let zero = scale.at(0.0);
        let top = scale.at(value);
        Self {
            x,
            y: top.min(zero),
            width: width.max(0.0),
            height: (top - zero).abs(),
        }
    }

    /// A square mark centred on a point.
    #[must_use]
    pub fn point(x: f64, y: f64, size: f64) -> Self {
        let half = size / 2.0;
        Self {
            x: x - half,
            y: y - half,
            width: size,
            height: size,
        }
    }

    /// The whole plot, which is the box the marks that are one shape for a whole series take.
    #[must_use]
    pub fn whole(plot: &Plot) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: plot.width,
            height: plot.height,
        }
    }

    /// The mark's outline, in its own box's coordinates.
    #[must_use]
    pub fn path(&self) -> String {
        rect_path(0.0, 0.0, self.width, self.height)
    }
}

/// A polyline through `points`, as path notation.
///
/// Empty for fewer than two points: a line through one point is a point, and a path with a single
/// move in it draws nothing while still costing a draw call.
#[must_use]
pub fn line_path(points: &[(f64, f64)]) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let mut path = String::with_capacity(points.len() * 16);
    for (index, (x, y)) in points.iter().enumerate() {
        let command = if index == 0 { 'M' } else { 'L' };
        path.push_str(&format!("{command}{x:.2} {y:.2} "));
    }
    path.trim_end().to_owned()
}

/// The area between `points` and the value axis' zero, as one closed path.
#[must_use]
pub fn area_path(points: &[(f64, f64)], baseline: f64) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let mut path = line_path(points);
    let last = points[points.len() - 1].0;
    let first = points[0].0;
    path.push_str(&format!(
        " L{last:.2} {baseline:.2} L{first:.2} {baseline:.2} Z"
    ));
    path
}

/// The axes and every grid line, as one path.
///
/// One path rather than one per line, because the grid is one thing to a reader and because a chart
/// with a dozen grid lines would otherwise be a dozen elements nobody can interact with, each with
/// its own style resolution and its own draw call.
#[must_use]
pub fn axes_path(plot: &Plot, scale: &Scale, ticks: &[f64]) -> String {
    let mut path = String::new();
    for tick in ticks {
        let y = scale.at(*tick);
        path.push_str(&format!(
            "M{:.2} {y:.2} L{:.2} {y:.2} ",
            plot.x0(),
            plot.x1()
        ));
    }
    // The two axes themselves, last, so they are drawn over the grid.
    path.push_str(&format!(
        "M{:.2} {:.2} L{:.2} {:.2} M{:.2} {:.2} L{:.2} {:.2}",
        plot.x0(),
        plot.y0(),
        plot.x0(),
        plot.y1(),
        plot.x0(),
        plot.y1(),
        plot.x1(),
        plot.y1(),
    ));
    path
}

#[cfg(test)]
mod tests {
    use super::{MarkBox, Plot, area_path, axes_path, line_path, rect_path};

    /// Every coordinate in a path string, in order.
    fn numbers(path: &str) -> Vec<f64> {
        path.split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .filter(|piece| !piece.is_empty())
            .filter_map(|piece| piece.parse().ok())
            .collect()
    }

    #[test]
    fn a_rectangle_is_four_corners_and_a_close() {
        let path = rect_path(10.0, 20.0, 30.0, 40.0);
        assert!(path.starts_with("M10.00 20.00"));
        assert!(path.ends_with('Z'));
        assert_eq!(numbers(&path).len(), 8, "four corners");
    }

    #[test]
    fn a_bar_grows_from_zero_in_whichever_direction_its_value_goes() {
        let plot = Plot::new(200.0, 100.0);
        let scale = plot.value_scale([-50.0, 100.0]);
        let zero = scale.at(0.0);

        let up = MarkBox::bar(0.0, 10.0, 100.0, &scale);
        assert!(up.y < zero, "a positive bar's top is above zero");
        let down = MarkBox::bar(0.0, 10.0, -50.0, &scale);
        assert!(
            (down.y - zero).abs() < 0.01,
            "a negative bar starts at zero and goes down",
        );
    }

    #[test]
    fn a_bar_of_zero_has_no_height_and_still_produces_a_path() {
        let plot = Plot::new(200.0, 100.0);
        let scale = plot.value_scale([0.0, 100.0]);
        let bar = MarkBox::bar(0.0, 10.0, 0.0, &scale);
        assert_eq!(bar.height, 0.0);
        let corners = numbers(&bar.path());
        assert_eq!(corners[1], corners[5], "top and bottom are the same line");
    }

    #[test]
    fn two_bars_of_one_chart_are_two_boxes_rather_than_two_drawings_in_one() {
        // The whole reason a mark is an element: a pointer is over one measurement, and a box that
        // covered the plot would mean the last bar drawn was the only one anything could reach.
        let plot = Plot::new(300.0, 150.0);
        let scale = plot.value_scale([0.0, 200.0]);
        let (first_x, band) = plot.band(0, 3);
        let (second_x, _) = plot.band(1, 3);
        let first = MarkBox::bar(first_x, band, 120.0, &scale);
        let second = MarkBox::bar(second_x, band, 180.0, &scale);

        assert!(
            first.x + first.width <= second.x,
            "the boxes overlap, so one bar sits on top of the other",
        );
        assert!(first.width < plot.width, "a bar is not the whole plot");
        assert_ne!(first.height, second.height);
    }

    #[test]
    fn a_line_through_fewer_than_two_points_draws_nothing_rather_than_a_dot() {
        assert_eq!(line_path(&[]), "");
        assert_eq!(line_path(&[(1.0, 2.0)]), "");
        assert_eq!(
            line_path(&[(1.0, 2.0), (3.0, 4.0)]),
            "M1.00 2.00 L3.00 4.00"
        );
    }

    #[test]
    fn an_area_closes_back_to_the_baseline() {
        let path = area_path(&[(0.0, 10.0), (10.0, 20.0)], 100.0);
        assert!(path.ends_with('Z'));
        assert!(path.contains("L10.00 100.00"), "{path}");
        assert!(path.contains("L0.00 100.00"), "{path}");
    }

    #[test]
    fn a_point_mark_is_centred_on_the_point_it_marks() {
        let mark = MarkBox::point(50.0, 50.0, 8.0);
        assert_eq!((mark.x, mark.y), (46.0, 46.0));
        assert_eq!((mark.width, mark.height), (8.0, 8.0));
    }

    #[test]
    fn the_axes_and_the_grid_are_one_path_with_a_line_per_tick_and_two_axes() {
        let plot = Plot::new(300.0, 200.0);
        let scale = plot.value_scale([0.0, 100.0]);
        let ticks = scale.ticks(4);
        let path = axes_path(&plot, &scale, &ticks);
        assert_eq!(
            path.matches('M').count(),
            ticks.len() + 2,
            "one move per grid line, and one per axis",
        );
    }

    #[test]
    fn the_bands_of_a_bar_chart_tile_the_plot_exactly() {
        let plot = Plot::new(300.0, 200.0);
        let (first, width) = plot.band(0, 6);
        let (last, _) = plot.band(5, 6);
        assert!((first - plot.x0()).abs() < 1e-9);
        assert!(
            (last + width - plot.x1()).abs() < 1e-9,
            "the last band ends at the plot's right edge",
        );
    }

    #[test]
    fn a_plot_smaller_than_its_own_margins_does_not_invert() {
        let tiny = Plot::new(10.0, 10.0);
        assert!(tiny.inner_width() >= 0.0);
        assert!(tiny.inner_height() >= 0.0);
    }
}
