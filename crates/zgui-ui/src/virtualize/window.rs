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
/// # A list longer than a container can be
///
/// The extent is capped at [`MAX_EXTENT`]. Past that the container is made exactly that tall and
/// the scroll position maps onto the rows rather than mirroring them, so a scrollbar drag moves by
/// many rows at once. Everything else — the keys, the built window, the row heights — is unchanged,
/// and [`VirtualWindow::offset_of`] is where a row sits either way.
///
/// [`lead`]: VirtualWindow::lead
/// [`trail`]: VirtualWindow::trail
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct VirtualWindow {
    /// How many rows the list has, built and unbuilt alike.
    pub rows: usize,
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
    /// it would be if every row existed — or [`MAX_EXTENT`] for a list too long to say.
    #[must_use]
    pub fn total(&self) -> f32 {
        self.lead + self.count as f32 * self.row_size + self.trail
    }

    /// Whether the container stands for more rows than its height can hold one by one.
    ///
    /// A compressed list scrolls in jumps: one pixel of the bar is many rows. What a view asks
    /// before telling somebody the bar is exact.
    #[must_use]
    pub fn is_compressed(&self) -> bool {
        f64::from(self.row_size) * self.rows as f64 > f64::from(MAX_EXTENT)
    }

    /// Where row `index` sits in the container, in CSS pixels.
    ///
    /// `index * row_size` for an ordinary list, and where the row falls in the compressed extent
    /// for one too long for that. What anything scrolling a row into view asks, because a position
    /// worked out from the row height alone is off by the whole compression.
    #[must_use]
    pub fn offset_of(&self, index: usize) -> f32 {
        if !self.is_compressed() {
            return index as f32 * self.row_size;
        }
        let last = self.rows.saturating_sub(1);
        if last == 0 {
            return 0.0;
        }
        let fraction = (index.min(last) as f64) / last as f64;
        (fraction * f64::from(MAX_EXTENT)) as f32
    }
}

/// The tallest a virtualised container is ever made, in CSS pixels.
///
/// Past about sixteen million a single-precision pixel can no longer tell one integer from the
/// next, and a damage rectangle is counted in `i32` device pixels. A container measured in billions
/// — which a multi-gigabyte file read sixteen bytes to the row asks for — lays out wrong and paints
/// nothing. Four million is far inside both, and still longer than any bar somebody drags.
pub const MAX_EXTENT: f32 = 4_000_000.0;

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
            rows,
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

    // A list whose own height is more than a container can be. The offset then names a fraction of
    // the list rather than a number of rows, and the two spacers add up to the capped extent.
    let full = rows as f64 * f64::from(row_size);
    if full > f64::from(MAX_EXTENT) {
        let extent = f64::from(MAX_EXTENT);
        let fraction = (f64::from(offset) / extent).clamp(0.0, 1.0);
        let anchor = ((fraction * (rows - 1) as f64) as usize).min(rows - 1);

        let first = anchor.saturating_sub(overscan);
        let end = anchor
            .saturating_add(visible)
            .saturating_add(overscan)
            .min(rows);
        let count = end - first;

        // The built rows sit under the port: the anchor is `overscan` rows into them, and the port
        // begins at the offset.
        let body = count as f64 * f64::from(row_size);
        let room = (extent - body).max(0.0);
        let lead = (f64::from(offset) - overscan as f64 * f64::from(row_size)).clamp(0.0, room);
        return VirtualWindow {
            rows,
            first,
            count,
            lead: lead as f32,
            trail: (extent - body - lead).max(0.0) as f32,
            row_size,
        };
    }

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
        rows,
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

    /// Three point two gigabytes of hex, sixteen bytes to a row: what broke before the cap.
    const HUGE: usize = 3_221_225_472 / 16;

    #[test]
    fn a_list_longer_than_a_container_can_be_is_capped() {
        let seen = window(HUGE, 22.0, 700.0, 0.0, 6);
        assert!(seen.is_compressed());
        assert!(
            seen.total() <= super::MAX_EXTENT + 1.0,
            "the container measured {}",
            seen.total()
        );
        assert!(
            seen.count < 100,
            "and it still builds a handful: {}",
            seen.count
        );
    }

    #[test]
    fn a_compressed_list_reaches_its_last_row() {
        // The whole point: the end of a very long list has to be somewhere the bar can go.
        let seen = window(HUGE, 22.0, 700.0, super::MAX_EXTENT, 6);
        assert!(seen.contains(HUGE - 1), "the last row is not built");
        assert_eq!(seen.end(), HUGE);
    }

    #[test]
    fn a_compressed_window_walks_the_whole_list_in_order() {
        let mut last = 0;
        for step in 0..=100 {
            let offset = super::MAX_EXTENT * step as f32 / 100.0;
            let seen = window(HUGE, 22.0, 700.0, offset, 6);
            assert!(seen.first >= last, "the window went backwards");
            assert!(seen.total() <= super::MAX_EXTENT + 1.0);
            assert!(seen.count > 0);
            last = seen.first;
        }
        assert!(last > HUGE - 100, "the walk never reached the end: {last}");
    }

    #[test]
    fn where_a_row_sits_is_the_row_height_until_it_cannot_be() {
        // An ordinary list is exact, which is what a caret scrolled into view depends on.
        let ordinary = window(10_000, 20.0, 400.0, 0.0, 2);
        assert!(!ordinary.is_compressed());
        assert_eq!(ordinary.offset_of(500), 10_000.0);

        // A compressed one answers where the row falls in the extent instead, and the two ends
        // are the two ends.
        let huge = window(HUGE, 22.0, 700.0, 0.0, 6);
        assert_eq!(huge.offset_of(0), 0.0);
        assert!((huge.offset_of(HUGE - 1) - super::MAX_EXTENT).abs() < 1.0);
        assert!(huge.offset_of(HUGE / 2) > 0.0);
    }

    #[test]
    fn a_list_just_under_the_cap_is_not_compressed() {
        // The boundary is where the two branches meet, and the ordinary one must keep its exact
        // arithmetic right up to it.
        let rows = (super::MAX_EXTENT / 20.0) as usize;
        let under = window(rows, 20.0, 400.0, 0.0, 2);
        assert!(!under.is_compressed());
        assert!((under.total() - super::MAX_EXTENT).abs() < 1.0);
        assert_eq!(under.offset_of(7), 140.0, "still exact");
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
