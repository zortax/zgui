//! A fragment kind with a painter behind it and no producer at all.

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
///
/// The gutter is reserved and nothing is ever put in it: the scrollbar arm above is live code that
/// no document reaches.
pub fn kinds(_scrolls: bool) -> Vec<FragmentKind> {
    vec![FragmentKind::Box]
}
