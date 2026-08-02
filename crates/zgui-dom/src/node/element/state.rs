//! Interaction state, and the proof that two crates mean the same bits by it.
//!
//! An element's interaction state is one 64-bit word and it is the single source of truth for the
//! state pseudo-classes — `:hover`, `:checked`, `:disabled`, `:dir()` and thirty more. There is no
//! second hover set and no second focus set anywhere in this framework; input routing writes these
//! bits and selector matching reads them.
//!
//! The word has two names. Callers above the document speak [`UiState`], which carries no engine
//! vocabulary; the style engine speaks its own [`ElementState`]. They are the same bits in the same
//! positions, which makes the conversion a reinterpretation rather than a translation — and that
//! is only true for as long as it stays true. So this module states the correspondence one bit at a
//! time, at compile time. A bit that moves on either side stops the build instead of quietly
//! matching the wrong pseudo-class, which is a bug with no stack trace.

use stylo_dom::ElementState;
use zgui_vocab::UiState;

/// The style engine's form of an interaction state.
pub const fn to_engine(state: UiState) -> ElementState {
    ElementState::from_bits_retain(state.bits())
}

/// The vocabulary form of an interaction state.
pub const fn from_engine(state: ElementState) -> UiState {
    UiState::from_bits(state.bits())
}

/// Asserts that one named state occupies the same bit on both sides.
macro_rules! same_bit {
    ($($ours:ident == $theirs:ident),* $(,)?) => {$(
        const _: () = assert!(UiState::$ours.bits() == ElementState::$theirs.bits());
    )*};
}

same_bit! {
    ACTIVE == ACTIVE,
    FOCUS == FOCUS,
    HOVER == HOVER,
    ENABLED == ENABLED,
    DISABLED == DISABLED,
    CHECKED == CHECKED,
    INDETERMINATE == INDETERMINATE,
    PLACEHOLDER_SHOWN == PLACEHOLDER_SHOWN,
    URL_TARGET == URLTARGET,
    FULLSCREEN == FULLSCREEN,
    VALID == VALID,
    INVALID == INVALID,
    USER_VALID == USER_VALID,
    USER_INVALID == USER_INVALID,
    BROKEN == BROKEN,
    REQUIRED == REQUIRED,
    OPTIONAL == OPTIONAL_,
    DEFINED == DEFINED,
    VISITED == VISITED,
    UNVISITED == UNVISITED,
    DRAG_OVER == DRAGOVER,
    IN_RANGE == INRANGE,
    OUT_OF_RANGE == OUTOFRANGE,
    READ_ONLY == READONLY,
    READ_WRITE == READWRITE,
    DEFAULT == DEFAULT,
    OPTIMUM == OPTIMUM,
    SUB_OPTIMUM == SUB_OPTIMUM,
    SUB_SUB_OPTIMUM == SUB_SUB_OPTIMUM,
    INCREMENT_SCRIPT_LEVEL == INCREMENT_SCRIPT_LEVEL,
    FOCUS_RING == FOCUSRING,
    FOCUS_WITHIN == FOCUS_WITHIN,
    LTR == LTR,
    RTL == RTL,
    HAS_DIR_ATTR == HAS_DIR_ATTR,
    HAS_DIR_ATTR_LTR == HAS_DIR_ATTR_LTR,
    HAS_DIR_ATTR_RTL == HAS_DIR_ATTR_RTL,
    HAS_DIR_ATTR_LIKE_AUTO == HAS_DIR_ATTR_LIKE_AUTO,
    AUTOFILL == AUTOFILL,
    AUTOFILL_PREVIEW == AUTOFILL_PREVIEW,
    MODAL == MODAL,
    INERT == INERT,
    TOPMOST_MODAL == TOPMOST_MODAL,
    DEVTOOLS_HIGHLIGHTED == DEVTOOLS_HIGHLIGHTED,
    STYLE_EDITOR_TRANSITIONING == STYLEEDITOR_TRANSITIONING,
    VALUE_EMPTY == VALUE_EMPTY,
    REVEALED == REVEALED,
    POPOVER_OPEN == POPOVER_OPEN,
    HAS_SLOTTED == HAS_SLOTTED,
    OPEN == OPEN,
    ACTIVE_VIEW_TRANSITION == ACTIVE_VIEW_TRANSITION,
    SUPPRESS_FOR_PRINT_SELECTION == SUPPRESS_FOR_PRINT_SELECTION,
    PAUSED == PAUSED,
    SEEKING == SEEKING,
    BUFFERING == BUFFERING,
    STALLED == STALLED,
    MUTED == MUTED,
    FULLSCREEN_KEYBOARD_LOCK == FULLSCREEN_KEYBOARD_LOCK,
    PICTURE_IN_PICTURE == PICTURE_IN_PICTURE,
}

// The heading level is a four-bit field rather than a flag, and both sides pack it in the same
// place.
const _: () = assert!(UiState::HEADING_LEVEL.bits() == ElementState::HEADING_LEVEL_BITS.bits());
const _: () = assert!(UiState::HEADING_LEVEL_OFFSET as usize == stylo_dom::HEADING_LEVEL_OFFSET);

// The unions each side offers for its own convenience cover the same bits.
const _: () = assert!(UiState::VALIDITY.bits() == ElementState::VALIDITY_STATES.bits());
const _: () = assert!(UiState::DIRECTION.bits() == ElementState::DIR_STATES.bits());
const _: () = assert!(UiState::LINK.bits() == ElementState::VISITED_OR_UNVISITED.bits());
const _: () = assert!(UiState::DIRECTION_ATTR.bits() == ElementState::DIR_ATTR_STATES.bits());
const _: () = assert!(UiState::GAUGE.bits() == ElementState::METER_OPTIMUM_STATES.bits());

// Both words are the same width, so a reinterpretation cannot lose a bit off the top.
const _: () = assert!(size_of::<UiState>() == size_of::<ElementState>());

#[cfg(test)]
mod tests {
    use stylo_dom::ElementState;
    use zgui_vocab::UiState;

    use super::{from_engine, to_engine};

    #[test]
    fn a_state_survives_the_round_trip_in_both_directions() {
        let ours = UiState::HOVER | UiState::CHECKED | UiState::LTR;
        assert_eq!(from_engine(to_engine(ours)), ours);

        let theirs = ElementState::FOCUSRING | ElementState::OPTIONAL_;
        assert_eq!(to_engine(from_engine(theirs)), theirs);
    }

    #[test]
    fn the_names_that_differ_still_mean_the_same_bit() {
        assert_eq!(to_engine(UiState::URL_TARGET), ElementState::URLTARGET);
        assert_eq!(to_engine(UiState::OPTIONAL), ElementState::OPTIONAL_);
        assert_eq!(to_engine(UiState::FOCUS_RING), ElementState::FOCUSRING);
        assert_eq!(to_engine(UiState::READ_ONLY), ElementState::READONLY);
    }

    #[test]
    fn a_heading_level_survives_the_crossing() {
        let ours = UiState::with_heading_level(3);
        assert_eq!(from_engine(to_engine(ours)).heading_level(), Some(3));
    }
}
