//! How a scroll container draws its content in a coordinate system of its own.
//!
//! A scroll container gives its children a space that its scroll offset moves, the first time it
//! scrolls. A scroll frame then writes that space's matrix and leaves every piece below the
//! container as it is: their rectangles, clip chains and hit entries are measured in the content
//! space, which the scroll does not change.
//!
//! Two kinds of box do not move with the content. A `position: fixed` box hangs from the space
//! above the content space, so the scroll does not move it. A sticky box is measured against the
//! port, so a scroll composes it again against the moved port. A scroll frame visits both, and the
//! path down to each, so the subtree inks above them stay true.
//!
//! A container keeps its children in its own space when it does not clip them, when it is drawn
//! through a matrix, or when a fixed box below hangs from a transformed or sticky box below it,
//! because the content space would move that box. A scroll of such a container changes the shift
//! its children are composed against, and the frame draws the port again instead of moving its
//! pixels. Under a matrix, the fragment pass composes every child again. Otherwise it carries each
//! rigid child as a layout move and composes the others again.
//!
//! The decision is made per container in the fragment pass; see
//! [`diff`](crate::fragment::diff).
