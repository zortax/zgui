//! What a scrolled container owes the rest of the frame.
//!
//! One bit for the fragment pass and one for the accessibility projection, on the container and on
//! nothing else. Everything a scroll is supposed to cost follows from how short that list is, so it
//! is written down once, here, rather than at each of the places a scroll can start.

use zgui_bits::Dirty;
use zgui_dom::Document;
use zgui_dom::NodeKey;

/// What one container that moved owes.
///
/// `SCROLL` is the bit the fragment pass is entered on, and it is what makes that pass descend
/// through the container and recompose its descendants against the new offset. `A11Y` is what the
/// accessibility projection reads, and one node is the whole of what a scroll changes there,
/// because every descendant's bounds are published relative to the container.
///
/// Neither `RELAYOUT` nor `RESTYLE` nor `REPAINT` is here, and their absence is the design rather
/// than an omission: the descendants that move are recomposed by the pass this bit lets in, and
/// each one's own `REPOSITION` is written by that pass from the geometry it compares. Marking a
/// subtree here would be marking, one node at a time, precisely what the pass is about to discover
/// — and it would turn a scroll of a five-thousand-row list from one mark into five thousand.
pub const SCROLLED: Dirty = Dirty::SCROLL.union(Dirty::A11Y);

/// Marks every container that moved, answering how many marks were actually written.
///
/// A container that has since left the document is skipped rather than guessed at: invalidation is
/// recorded against a live node, and a scroll whose element was removed between the wheel event and
/// the mark has nothing to record it on.
pub fn scrolled(document: &mut Document, containers: &[NodeKey]) -> usize {
    let mut marked = 0;
    for container in containers {
        let Some(index) = document.store().index_of(*container) else {
            continue;
        };
        zgui_dom::dirty::propagate::mark(document.store_mut(), index, SCROLLED);
        marked += 1;
    }
    marked
}

#[cfg(test)]
mod tests {
    use zgui_bits::Dirty;

    use super::SCROLLED;

    #[test]
    fn a_scroll_owes_the_fragment_pass_and_the_accessibility_tree_and_nothing_else() {
        assert!(SCROLLED.contains(Dirty::SCROLL));
        assert!(SCROLLED.contains(Dirty::A11Y));
        for owed in [
            Dirty::RESTYLE,
            Dirty::RECASCADE,
            Dirty::RELAYOUT,
            Dirty::REBUILD_BOX,
            Dirty::RESHAPE,
            Dirty::REPAINT,
            Dirty::RESTACK,
        ] {
            assert!(
                !SCROLLED.intersects(owed),
                "a scroll must not owe {owed:?}: everything above the fragment pass would then run \
                 once per notch of the wheel"
            );
        }
    }
}
