//! A fragment kind whose every variant is both branched on and built.

/// What a fragment draws.
pub enum FragmentKind {
    /// A box's own decorations.
    Box,
    /// Part of a scrollbar.
    Scrollbar,
}

/// What one fragment draws, in primitives.
pub fn emit(kind: FragmentKind) -> usize {
    match kind {
        FragmentKind::Box => 0,
        FragmentKind::Scrollbar => 1,
    }
}

/// What each fragment of one scrolling box draws.
pub fn kinds(scrolls: bool) -> Vec<FragmentKind> {
    let mut kinds = vec![FragmentKind::Box];
    if scrolls {
        kinds.push(FragmentKind::Scrollbar);
    }
    kinds
}
