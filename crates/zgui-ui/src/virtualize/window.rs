//! Which rows of a long list are worth building, and how much space stands in for the rest.

/// The rows a virtualised list builds, and the space that replaces the ones it did not.
///
/// A list of ten thousand rows is ten thousand elements to style, lay out, paint and hit-test, and
/// a scrollport thirty of them tall never shows more than thirty. This is the answer to *which
/// thirty*: a contiguous run, plus the two lengths that keep the scrollbar honest — [`lead`] is the
/// height of everything before the run and [`trail`] the height of everything after it, so the
/// container measures the full list while holding a fraction of it.
///
/// Every length here is in CSS pixels, because that is what a style sheet is written in and what
/// the row height was declared as. The scroll position that produced it is in device pixels, and
/// converting between them is [`Virtualize`](crate::virtualize::Virtualize)'s job rather than this
/// type's.
///
/// ```
/// use zgui_ui::virtualize::{VirtualWindow, window};
///
/// // Ten thousand rows of 20px, a 400px port, scrolled to the thousandth row, two rows of slack.
/// let seen = window(10_000, 20.0, 400.0, 20_000.0, 2);
/// assert_eq!(seen.first, 998, "two rows of slack above the first visible one");
/// assert_eq!(seen.count, 25, "twenty on screen, one straddling the edge, four of slack");
/// assert_eq!(seen.lead, 19_960.0);
/// assert_eq!(seen.total(), 200_000.0, "the container still measures the whole list");
/// ```
///
/// [`lead`]: VirtualWindow::lead
/// [`trail`]: VirtualWindow::trail
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct VirtualWindow {
    /// The index of the first row that is built.
    pub first: usize,
    /// How many rows are built, counting from [`VirtualWindow::first`].
    pub count: usize,
    /// The height of everything before the first built row, in CSS pixels.
    pub lead: f32,
    /// The height of everything after the last built row, in CSS pixels.
    pub trail: f32,
    /// How tall one row is, in CSS pixels.
    ///
    /// Carried so that [`VirtualWindow::total`] can answer, and because a consumer laying the rows
    /// out needs the same number the window was decided from — two copies of it are two numbers
    /// that disagree on the frame one of them changes.
    pub row_size: f32,
}

impl VirtualWindow {
    /// The indices that are built, in order.
    ///
    /// What a keyed list is driven by: the key is the row's own index, so a scroll that moves the
    /// window by one row destroys one row and builds one row and leaves the rest untouched.
    #[must_use]
    pub fn indices(&self) -> Vec<usize> {
        (self.first..self.first + self.count).collect()
    }

    /// Whether `index` is one of the rows that is built.
    #[must_use]
    pub const fn contains(&self, index: usize) -> bool {
        index >= self.first && index < self.first + self.count
    }

    /// The index one past the last built row.
    #[must_use]
    pub const fn end(&self) -> usize {
        self.first + self.count
    }

    /// The height of the whole list, built and unbuilt alike, in CSS pixels.
    ///
    /// The two spacers plus the rows between them, which is what makes the scrollbar the same size
    /// it would be if every row existed.
    #[must_use]
    pub fn total(&self) -> f32 {
        self.lead + self.count as f32 * self.row_size + self.trail
    }
}

/// The rows worth building for a list of `rows` rows, each `row_size` CSS pixels tall.
///
/// `viewport` is how tall the scrollport is and `offset` how far it has been scrolled, both in CSS
/// pixels. `overscan` is how many rows to build beyond each edge, which is what stops a fast scroll
/// showing a band of nothing before the next frame catches up.
///
/// # The two degenerate inputs
///
/// A row height of zero or less would divide the list into infinitely many rows, so it is treated
/// as a list that has not been measured: one row is built, and the next frame — which has a
/// measurement — builds the right ones.
///
/// A viewport of zero is the *ordinary* state on the first frame, because nothing has been laid out
/// yet and the observation that reports the scrollport's size has not fired. Building nothing there
/// would be a list that stays empty for ever in a container whose height comes from its content, so
/// a viewport that small is rounded up to one row.
///
/// ```
/// use zgui_ui::virtualize::window;
///
/// // Before the first layout: no measurement, so one row plus the slack, and never nothing.
/// let unmeasured = window(500, 0.0, 0.0, 0.0, 1);
/// assert!(unmeasured.count >= 1 && unmeasured.count <= 4);
///
/// // A list shorter than its port is not virtualised at all.
/// let short = window(3, 20.0, 400.0, 0.0, 2);
/// assert_eq!((short.first, short.count), (0, 3));
/// assert_eq!((short.lead, short.trail), (0.0, 0.0));
/// ```
#[must_use]
pub fn window(
    rows: usize,
    row_size: f32,
    viewport: f32,
    offset: f32,
    overscan: usize,
) -> VirtualWindow {
    if rows == 0 {
        return VirtualWindow::default();
    }
    if !row_size.is_finite() || row_size <= 0.0 {
        // Unmeasured rather than empty: see the note above. One row plus the slack is bounded, and
        // it gives the container something to measure so that the next frame knows better.
        let count = (1 + overscan).min(rows);
        return VirtualWindow {
            first: 0,
            count,
            lead: 0.0,
            trail: 0.0,
            row_size: 0.0,
        };
    }

    let offset = if offset.is_finite() {
        offset.max(0.0)
    } else {
        0.0
    };
    let viewport = if viewport.is_finite() {
        viewport.max(0.0)
    } else {
        0.0
    };

    // Rounded up, plus one: a port 400px tall showing 20px rows meets twenty-one rows whenever it
    // is scrolled to anything but an exact multiple of the row height.
    let visible = ((viewport / row_size).ceil() as usize).max(1) + 1;
    let anchor = (offset / row_size).floor().max(0.0);
    let anchor = if anchor >= rows as f32 {
        rows - 1
    } else {
        anchor as usize
    };

    let first = anchor.saturating_sub(overscan);
    let end = anchor
        .saturating_add(visible)
        .saturating_add(overscan)
        .min(rows);
    let count = end - first;

    VirtualWindow {
        first,
        count,
        lead: first as f32 * row_size,
        trail: (rows - end) as f32 * row_size,
        row_size,
    }
}

#[cfg(test)]
mod tests {
    use super::{VirtualWindow, window};

    #[test]
    fn the_spacers_and_the_rows_always_add_up_to_the_whole_list() {
        let rows = 10_000;
        let row_size = 24.0;
        for step in 0..200 {
            let offset = step as f32 * 1_197.0;
            let seen = window(rows, row_size, 600.0, offset, 3);
            let measured = seen.total();
            assert!(
                (measured - rows as f32 * row_size).abs() < 0.01,
                "the container would measure {measured} instead of the whole list",
            );
        }
    }

    #[test]
    fn a_window_is_bounded_however_long_the_list_is() {
        // Both scrolled well clear of either end, so neither is clamped: the only difference
        // between them is how much data there is behind the port.
        let small = window(100, 20.0, 400.0, 500.0, 2);
        let huge = window(1_000_000, 20.0, 400.0, 500_000.0, 2);
        assert_eq!(
            small.count, huge.count,
            "the number of built rows is a function of the port, not of the list",
        );
        assert_eq!(huge.count, 25, "and it is the port's own handful");
    }

    #[test]
    fn scrolling_less_than_one_row_does_not_move_the_window() {
        let before = window(10_000, 20.0, 400.0, 400.0, 2);
        let after = window(10_000, 20.0, 400.0, 419.0, 2);
        assert_eq!(before, after, "a sub-row scroll rebuilds nothing");
        let next = window(10_000, 20.0, 400.0, 420.0, 2);
        assert_eq!(
            next.first,
            before.first + 1,
            "and a full row moves it by one"
        );
    }

    #[test]
    fn the_end_of_the_list_is_not_overrun() {
        let last = window(50, 20.0, 400.0, 10_000.0, 4);
        assert_eq!(last.end(), 50);
        assert_eq!(last.trail, 0.0);
        assert!(last.contains(49));
    }

    #[test]
    fn an_empty_list_builds_nothing() {
        assert_eq!(window(0, 20.0, 400.0, 0.0, 2), VirtualWindow::default());
        assert!(window(0, 20.0, 400.0, 0.0, 2).indices().is_empty());
    }

    #[test]
    fn a_row_height_of_zero_is_treated_as_a_list_nobody_has_measured() {
        let unmeasured = window(500, 0.0, 400.0, 0.0, 2);
        assert!(
            (1..=3).contains(&unmeasured.count),
            "an unmeasured list builds a bounded handful, never all five hundred",
        );
    }

    #[test]
    fn a_scroll_past_the_end_still_names_rows_that_exist() {
        let past = window(10, 20.0, 400.0, 1_000_000.0, 2);
        assert!(past.end() <= 10);
        assert!(past.count > 0);
    }

    #[test]
    fn the_indices_are_the_rows_the_window_claims() {
        let seen = window(100, 10.0, 50.0, 300.0, 1);
        let indices = seen.indices();
        assert_eq!(indices.first().copied(), Some(seen.first));
        assert_eq!(indices.len(), seen.count);
        assert!(indices.iter().all(|index| seen.contains(*index)));
    }
}
