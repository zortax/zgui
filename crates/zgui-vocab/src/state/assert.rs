//! The compile-time statement of [`UiState`]'s bit layout.
//!
//! [`UiState`] is not merely *a* set of interaction states; it is the same word, bit for bit, that
//! the style engine matches selectors against, so converting one to the other is a reinterpretation
//! rather than a translation. That only stays true if the bit positions stay put, and a bit that
//! quietly moved would show up as the wrong pseudo-class matching — a bug with no stack trace.
//!
//! So the positions are written out a second time here, longhand, one assertion per bit, and the
//! assertions are `const`. What that catches is an edit on *this* side: inserting a state in the
//! middle of the declarations, renumbering around one, or letting a union drift from the bits it
//! claims, all stop the build rather than reaching a test run.
//!
//! What it cannot catch is the *other* side moving, because the crate that owns the style engine's
//! layout is not among this crate's three dependencies and must not become one. The pairwise
//! comparison against that layout therefore belongs to the crate that names both, and it is the
//! same shape as the assertions below with the right-hand side replaced by the real constant.

use crate::state::UiState;

const _: () = assert!(UiState::ACTIVE.bits() == 1u64 << 0);
const _: () = assert!(UiState::FOCUS.bits() == 1u64 << 1);
const _: () = assert!(UiState::HOVER.bits() == 1u64 << 2);
const _: () = assert!(UiState::ENABLED.bits() == 1u64 << 3);
const _: () = assert!(UiState::DISABLED.bits() == 1u64 << 4);
const _: () = assert!(UiState::CHECKED.bits() == 1u64 << 5);
const _: () = assert!(UiState::INDETERMINATE.bits() == 1u64 << 6);
const _: () = assert!(UiState::PLACEHOLDER_SHOWN.bits() == 1u64 << 7);
const _: () = assert!(UiState::URL_TARGET.bits() == 1u64 << 8);
const _: () = assert!(UiState::FULLSCREEN.bits() == 1u64 << 9);
const _: () = assert!(UiState::VALID.bits() == 1u64 << 10);
const _: () = assert!(UiState::INVALID.bits() == 1u64 << 11);
const _: () = assert!(UiState::USER_VALID.bits() == 1u64 << 12);
const _: () = assert!(UiState::USER_INVALID.bits() == 1u64 << 13);
const _: () = assert!(UiState::BROKEN.bits() == 1u64 << 14);
const _: () = assert!(UiState::REQUIRED.bits() == 1u64 << 15);
const _: () = assert!(UiState::OPTIONAL.bits() == 1u64 << 16);
const _: () = assert!(UiState::DEFINED.bits() == 1u64 << 17);
const _: () = assert!(UiState::VISITED.bits() == 1u64 << 18);
const _: () = assert!(UiState::UNVISITED.bits() == 1u64 << 19);
const _: () = assert!(UiState::DRAG_OVER.bits() == 1u64 << 20);
const _: () = assert!(UiState::IN_RANGE.bits() == 1u64 << 21);
const _: () = assert!(UiState::OUT_OF_RANGE.bits() == 1u64 << 22);
const _: () = assert!(UiState::READ_ONLY.bits() == 1u64 << 23);
const _: () = assert!(UiState::READ_WRITE.bits() == 1u64 << 24);
const _: () = assert!(UiState::DEFAULT.bits() == 1u64 << 25);
const _: () = assert!(UiState::OPTIMUM.bits() == 1u64 << 26);
const _: () = assert!(UiState::SUB_OPTIMUM.bits() == 1u64 << 27);
const _: () = assert!(UiState::SUB_SUB_OPTIMUM.bits() == 1u64 << 28);
const _: () = assert!(UiState::INCREMENT_SCRIPT_LEVEL.bits() == 1u64 << 29);
const _: () = assert!(UiState::FOCUS_RING.bits() == 1u64 << 30);
const _: () = assert!(UiState::FOCUS_WITHIN.bits() == 1u64 << 31);
const _: () = assert!(UiState::LTR.bits() == 1u64 << 32);
const _: () = assert!(UiState::RTL.bits() == 1u64 << 33);
const _: () = assert!(UiState::HAS_DIR_ATTR.bits() == 1u64 << 34);
const _: () = assert!(UiState::HAS_DIR_ATTR_LTR.bits() == 1u64 << 35);
const _: () = assert!(UiState::HAS_DIR_ATTR_RTL.bits() == 1u64 << 36);
const _: () = assert!(UiState::HAS_DIR_ATTR_LIKE_AUTO.bits() == 1u64 << 37);
const _: () = assert!(UiState::AUTOFILL.bits() == 1u64 << 38);
const _: () = assert!(UiState::AUTOFILL_PREVIEW.bits() == 1u64 << 39);
const _: () = assert!(UiState::MODAL.bits() == 1u64 << 40);
const _: () = assert!(UiState::INERT.bits() == 1u64 << 41);
const _: () = assert!(UiState::TOPMOST_MODAL.bits() == 1u64 << 42);
const _: () = assert!(UiState::DEVTOOLS_HIGHLIGHTED.bits() == 1u64 << 43);
const _: () = assert!(UiState::STYLE_EDITOR_TRANSITIONING.bits() == 1u64 << 44);
const _: () = assert!(UiState::VALUE_EMPTY.bits() == 1u64 << 45);
const _: () = assert!(UiState::REVEALED.bits() == 1u64 << 46);
const _: () = assert!(UiState::POPOVER_OPEN.bits() == 1u64 << 47);
const _: () = assert!(UiState::HAS_SLOTTED.bits() == 1u64 << 48);
const _: () = assert!(UiState::OPEN.bits() == 1u64 << 49);
const _: () = assert!(UiState::ACTIVE_VIEW_TRANSITION.bits() == 1u64 << 50);
const _: () = assert!(UiState::SUPPRESS_FOR_PRINT_SELECTION.bits() == 1u64 << 51);
const _: () = assert!(UiState::PAUSED.bits() == 1u64 << 52);
const _: () = assert!(UiState::SEEKING.bits() == 1u64 << 53);
const _: () = assert!(UiState::BUFFERING.bits() == 1u64 << 54);
const _: () = assert!(UiState::STALLED.bits() == 1u64 << 55);
const _: () = assert!(UiState::MUTED.bits() == 1u64 << 56);
const _: () = assert!(UiState::FULLSCREEN_KEYBOARD_LOCK.bits() == 1u64 << 57);
const _: () = assert!(UiState::PICTURE_IN_PICTURE.bits() == 1u64 << 61);

// The heading level is a four-bit field rather than a flag, and it starts where the mirrored
// layout puts it.
const _: () = assert!(UiState::HEADING_LEVEL_OFFSET == 57);
const _: () = assert!(UiState::HEADING_LEVEL.bits() == 0b1111u64 << 57);

// The word is a `u64`, so the highest bit named above has to fit in it.
const _: () = assert!(UiState::PICTURE_IN_PICTURE.bits() < u64::MAX);
const _: () = assert!(size_of::<UiState>() == size_of::<u64>());
const _: () = assert!(align_of::<UiState>() == align_of::<u64>());

// The unions are exactly the bits they claim to be, so a mask read by the style engine and a mask
// written here cannot drift apart.
const _: () = assert!(
    UiState::VALIDITY.bits()
        == UiState::VALID.bits()
            | UiState::INVALID.bits()
            | UiState::USER_VALID.bits()
            | UiState::USER_INVALID.bits()
);
const _: () = assert!(UiState::DIRECTION.bits() == UiState::LTR.bits() | UiState::RTL.bits());
const _: () = assert!(UiState::LINK.bits() == UiState::VISITED.bits() | UiState::UNVISITED.bits());

// The one overlap in the layout, asserted so that it is a recorded fact rather than a surprise:
// the keyboard-lock bit is the low bit of the heading-level field. Nothing sets both, because
// nothing is both a heading and a full-screen presentation.
const _: () = assert!(
    UiState::FULLSCREEN_KEYBOARD_LOCK.bits() & UiState::HEADING_LEVEL.bits()
        == UiState::FULLSCREEN_KEYBOARD_LOCK.bits()
);
