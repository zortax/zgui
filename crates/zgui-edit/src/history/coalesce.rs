//! Whether a change joins the one before it.

use crate::history::entry::{EditKind, Entry};

/// Whether `next` continues `last` and may be folded into it.
///
/// Three things have to hold, and each of them is a way an editor gets this wrong:
///
/// * **the same kind of change**, so a backspace never joins the letters it is deleting;
/// * **adjacency** — the new text starts exactly where the last one ended for typing, and ends
///   exactly where the last one started for backspacing — so moving the caret and typing again is
///   a second undo;
/// * **no line break in either**, so pressing return always leaves an undo boundary behind it,
///   which is where a person expects one undo to stop.
pub fn joins(last: &Entry, next: &Entry) -> bool {
    if last.kind != next.kind || !next.kind.coalesces() {
        return false;
    }
    if last.inserted.contains('\n')
        || next.inserted.contains('\n')
        || last.removed.contains('\n')
        || next.removed.contains('\n')
    {
        return false;
    }
    match next.kind {
        EditKind::Insert => {
            next.range.start == last.inserted_range().end && next.removed.is_empty()
        }
        EditKind::DeleteBackwards => next.range.end == last.range.start && next.inserted.is_empty(),
        EditKind::DeleteForwards => {
            next.range.start == last.range.start && next.inserted.is_empty()
        }
        EditKind::Replace => false,
    }
}

/// Folds `next` into `last`.
///
/// # Panics
///
/// Panics unless [`joins`] holds, because the fold is only meaningful for the three adjacencies it
/// checks and a fold of anything else would silently rewrite the text an undo restores.
pub fn fold(last: &mut Entry, next: Entry) {
    assert!(joins(last, &next), "only adjacent changes of one kind fold");
    match next.kind {
        EditKind::Insert => last.inserted.push_str(&next.inserted),
        EditKind::DeleteBackwards => {
            let mut removed = next.removed;
            removed.push_str(&last.removed);
            last.removed = removed;
            last.range = next.range.start..next.range.start + last.removed.len();
        }
        EditKind::DeleteForwards => {
            last.removed.push_str(&next.removed);
            last.range = last.range.start..last.range.start + last.removed.len();
        }
        _ => unreachable!("joins refused every other kind"),
    }
    last.after = next.after;
}

#[cfg(test)]
mod tests {
    use super::{fold, joins};
    use crate::history::entry::{EditKind, Entry};
    use crate::select::Selection;

    /// An entry for a change at `range` replacing `removed` with `inserted`.
    fn entry(
        range: core::ops::Range<usize>,
        removed: &str,
        inserted: &str,
        kind: EditKind,
    ) -> Entry {
        Entry {
            before: Selection::caret(range.start),
            after: Selection::caret(range.start + inserted.len()),
            range,
            removed: removed.to_owned(),
            inserted: inserted.to_owned(),
            kind,
        }
    }

    #[test]
    fn two_typed_letters_are_one_entry() {
        let mut first = entry(0..0, "", "a", EditKind::Insert);
        let second = entry(1..1, "", "b", EditKind::Insert);
        assert!(joins(&first, &second));
        fold(&mut first, second);
        assert_eq!(first.inserted, "ab");
        assert_eq!(first.after, Selection::caret(2));
    }

    #[test]
    fn typing_after_moving_the_caret_starts_a_second_entry() {
        let first = entry(0..0, "", "a", EditKind::Insert);
        let elsewhere = entry(9..9, "", "b", EditKind::Insert);
        assert!(!joins(&first, &elsewhere));
    }

    #[test]
    fn a_backspace_never_joins_the_typing_it_undid() {
        let typed = entry(0..0, "", "a", EditKind::Insert);
        let deleted = entry(0..1, "a", "", EditKind::DeleteBackwards);
        assert!(!joins(&typed, &deleted));
    }

    #[test]
    fn two_backspaces_are_one_entry_that_grows_backwards() {
        let mut first = entry(4..5, "e", "", EditKind::DeleteBackwards);
        let second = entry(3..4, "d", "", EditKind::DeleteBackwards);
        assert!(joins(&first, &second));
        fold(&mut first, second);
        assert_eq!(first.removed, "de");
        assert_eq!(first.range, 3..5);
    }

    #[test]
    fn two_forward_deletes_are_one_entry_that_grows_forwards() {
        // The range has to grow with the text it removed: it is what a redo replaces, so a range
        // left at its original length redoes half of the deletion and leaves the rest behind.
        let mut first = entry(2..3, "c", "", EditKind::DeleteForwards);
        let second = entry(2..3, "d", "", EditKind::DeleteForwards);
        assert!(joins(&first, &second));
        fold(&mut first, second);
        assert_eq!(first.removed, "cd");
        assert_eq!(first.range, 2..4);
    }

    #[test]
    fn a_line_break_always_leaves_an_undo_boundary() {
        let first = entry(0..0, "", "a", EditKind::Insert);
        let broken = entry(1..1, "", "\n", EditKind::Insert);
        assert!(!joins(&first, &broken));
    }

    #[test]
    fn a_paste_stands_alone_in_both_directions() {
        let typed = entry(0..0, "", "a", EditKind::Insert);
        let pasted = entry(1..1, "", "xyz", EditKind::Replace);
        assert!(!joins(&typed, &pasted));
        assert!(!joins(&pasted, &entry(4..4, "", "b", EditKind::Insert)));
    }
}
