//! Which page numbers to draw when there are more of them than there is room for.

/// One position in a rendered pager: a page number, or a gap where numbers were left out.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Slot {
    /// Draw this page number.
    Page(usize),
    /// Draw an ellipsis: the numbers here were left out.
    Gap,
}

/// Which numbers and gaps a pager of `slots` positions shows, for page `current` of `pages`.
///
/// A pure function of three numbers, so the rule can be read, tested and replaced without touching
/// a component. The rule is the usual one: the first and last page are always reachable, the pages
/// on either side of the current one are always shown, and whatever is skipped becomes one gap.
///
/// Both ends are one-based, because that is what a pager shows. `current` is clamped into range
/// rather than trusted, so a caller that has just deleted the last page draws something sensible
/// instead of nothing.
///
/// ```
/// use zgui_ui::pagination::{Slot, page_window};
///
/// // Everything fits: no gaps.
/// assert_eq!(
///     page_window(2, 4, 7),
///     [Slot::Page(1), Slot::Page(2), Slot::Page(3), Slot::Page(4)]
/// );
///
/// // In the middle of a long run: a gap on each side, and the ends still reachable.
/// assert_eq!(
///     page_window(10, 20, 7),
///     [
///         Slot::Page(1),
///         Slot::Gap,
///         Slot::Page(9),
///         Slot::Page(10),
///         Slot::Page(11),
///         Slot::Gap,
///         Slot::Page(20),
///     ]
/// );
///
/// // Near the start: one gap, and the run at the front is as long as the slots allow.
/// assert_eq!(page_window(1, 20, 5)[0], Slot::Page(1));
/// assert_eq!(page_window(1, 20, 5).last().copied(), Some(Slot::Page(20)));
/// ```
#[must_use]
pub fn page_window(current: usize, pages: usize, slots: usize) -> Vec<Slot> {
    if pages == 0 {
        return Vec::new();
    }
    let current = current.clamp(1, pages);
    // Fewer than five slots cannot hold two ends, two gaps and a current page, so a pager that
    // small simply lists what it can rather than drawing gaps that hide more than they save.
    let slots = slots.max(5);
    if pages <= slots {
        return (1..=pages).map(Slot::Page).collect();
    }

    // Two of the slots are the first and last page, and two more are the gaps that may be needed,
    // so the run between them is what is left. A pager is therefore never wider than it says.
    let run = slots - 4;
    let mut start = current.saturating_sub(run / 2).max(2);
    let mut end = (start + run - 1).min(pages - 1);
    // Pulling the end back may leave room at the front, which a run pressed against the last page
    // needs so that it is always the length it was asked for.
    start = end.saturating_sub(run - 1).max(2);

    // A gap that would hide exactly one page is drawn as that page instead: an ellipsis standing
    // for a single number is wider than the number and tells the reader less. It costs nothing,
    // because the slot the page takes is the one the gap gave up.
    if start == 3 {
        start = 2;
    }
    if end + 2 == pages {
        end = pages - 1;
    }

    let mut out = vec![Slot::Page(1)];
    if start > 2 {
        out.push(Slot::Gap);
    }
    out.extend((start..=end).map(Slot::Page));
    if end < pages - 1 {
        out.push(Slot::Gap);
    }
    out.push(Slot::Page(pages));
    out
}

#[cfg(test)]
mod tests {
    use super::{Slot, page_window};

    /// How many page numbers a window offers.
    fn numbers(slots: &[Slot]) -> Vec<usize> {
        slots
            .iter()
            .filter_map(|slot| match slot {
                Slot::Page(page) => Some(*page),
                Slot::Gap => None,
            })
            .collect()
    }

    #[test]
    fn the_page_you_are_on_is_always_offered() {
        for pages in [1_usize, 2, 7, 20, 101] {
            for current in 1..=pages {
                let window = page_window(current, pages, 7);
                assert!(
                    numbers(&window).contains(&current),
                    "page {current} of {pages} is not in {window:?}"
                );
            }
        }
    }

    #[test]
    fn the_first_and_last_page_are_always_reachable() {
        for current in 1..=40_usize {
            let window = page_window(current, 40, 7);
            let numbers = numbers(&window);
            assert_eq!(numbers.first().copied(), Some(1));
            assert_eq!(numbers.last().copied(), Some(40));
        }
    }

    #[test]
    fn the_numbers_only_ever_go_up_and_never_repeat() {
        for current in 1..=40_usize {
            let numbers = numbers(&page_window(current, 40, 7));
            assert!(
                numbers.windows(2).all(|pair| pair[0] < pair[1]),
                "{numbers:?} for page {current}"
            );
        }
    }

    #[test]
    fn a_gap_never_stands_for_a_single_page() {
        // The defect this catches is a pager that draws "1 … 3 4 5" where "1 2 3 4 5" is both
        // narrower and more useful.
        for pages in [9_usize, 10, 11, 25] {
            for current in 1..=pages {
                let window = page_window(current, pages, 7);
                for (index, slot) in window.iter().enumerate() {
                    if *slot != Slot::Gap {
                        continue;
                    }
                    let (Some(Slot::Page(before)), Some(Slot::Page(after))) =
                        (window.get(index - 1), window.get(index + 1))
                    else {
                        panic!("a gap is always between two numbers: {window:?}");
                    };
                    assert!(
                        after - before > 2,
                        "the gap in {window:?} hides only page {}",
                        before + 1
                    );
                }
            }
        }
    }

    #[test]
    fn a_pager_is_never_wider_than_it_was_asked_to_be() {
        // The point of the whole exercise: a row that stays the same width whichever page it is
        // showing, so nothing beside it moves as the user pages through a list.
        for slots in [5_usize, 6, 7, 9] {
            for pages in [8_usize, 12, 40, 500] {
                for current in 1..=pages {
                    let window = page_window(current, pages, slots);
                    assert!(
                        window.len() <= slots,
                        "page {current} of {pages} in {slots} slots drew {} of them",
                        window.len()
                    );
                }
            }
        }
    }

    #[test]
    fn a_pager_wide_enough_for_every_page_draws_every_page_and_no_gap() {
        assert_eq!(numbers(&page_window(3, 7, 7)), [1, 2, 3, 4, 5, 6, 7]);
        assert!(!page_window(3, 7, 7).contains(&Slot::Gap));
    }

    #[test]
    fn a_pager_with_nothing_to_page_through_draws_nothing() {
        assert!(page_window(1, 0, 7).is_empty());
    }

    #[test]
    fn a_page_outside_the_range_is_brought_into_it_rather_than_dropping_the_pager() {
        assert!(numbers(&page_window(99, 12, 7)).contains(&12));
        assert!(numbers(&page_window(0, 12, 7)).contains(&1));
    }
}
