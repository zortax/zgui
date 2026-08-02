//! What one descent of the offsetting walk is responsible for.
//!
//! A subtree that only moved owes two things that have nothing to do with each other: its pieces'
//! rectangles have to be moved, and the structures that say where its pieces are have to be told.
//! One descent normally does both, because they want the same recursion. This is how a descent is
//! told to do one of them, so that what each costs can be measured apart from the other — see
//! [`split`](crate::fragment::diff::split).
//!
//! Every duty is a type rather than a flag, so the choice is made once, at the call, and a descent
//! that is not asked for a duty contains no test for whether it was.

/// One descent's share of what moving a subtree owes.
pub(super) trait Duty {
    /// Whether this descent moves the rectangles and re-interns the clip chains.
    ///
    /// It is also the descent that counts the boxes: a subtree moved in three descents visits the
    /// same boxes as one moved in a single descent, and reporting each of them three times would
    /// make how the walk was measured show up as how much work the frame did.
    const MOVES: bool;

    /// Whether this descent moves the hit entries and marks the moved controls.
    const INDEXES: bool;
}

/// Everything, in one descent: what a frame that nobody is measuring makes.
pub(super) struct Both;

/// The rectangles and the clip chains alone.
pub(super) struct Geometry;

/// The hit entries and the accessibility marks alone.
pub(super) struct Index;

/// Neither — the bare traversal, which is the part the other two share.
pub(super) struct Skeleton;

impl Duty for Both {
    const MOVES: bool = true;
    const INDEXES: bool = true;
}

impl Duty for Geometry {
    const MOVES: bool = true;
    const INDEXES: bool = false;
}

impl Duty for Index {
    const MOVES: bool = false;
    const INDEXES: bool = true;
}

impl Duty for Skeleton {
    const MOVES: bool = false;
    const INDEXES: bool = false;
}
