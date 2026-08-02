//! The states that are logical negations of one another, and the write that keeps them so.

use crate::state::UiState;

/// The bits that must never be held together, and never both absent.
///
/// Each entry is the pair as it reads in prose: the positive state first, its negation second.
pub(crate) const COMPLEMENTS: &[(UiState, UiState)] = &[
    (UiState::ENABLED, UiState::DISABLED),
    (UiState::READ_WRITE, UiState::READ_ONLY),
    (UiState::VALID, UiState::INVALID),
    (UiState::OPTIONAL, UiState::REQUIRED),
];

impl UiState {
    /// The set with `state` added or removed, keeping every complementary pair consistent.
    ///
    /// Four pairs of states are negations of one another, and a set that holds both halves
    /// matches two contradictory selectors at once — an element that is `:enabled` *and*
    /// `:disabled`. Setting one half through this method clears the other; clearing one half
    /// sets the other. Every other state is set and cleared on its own.
    ///
    /// `state` may name more than one bit, in which case each is applied in turn.
    ///
    /// ```
    /// use zgui_vocab::UiState;
    ///
    /// let valid = UiState::VALID | UiState::ENABLED;
    /// let invalid = valid.apply(UiState::INVALID, true);
    /// assert!(invalid.contains(UiState::INVALID));
    /// assert!(!invalid.contains(UiState::VALID));
    ///
    /// // Clearing half a pair asserts the other half rather than leaving neither.
    /// let enabled = invalid.apply(UiState::DISABLED, false);
    /// assert!(enabled.contains(UiState::ENABLED));
    ///
    /// // A state with no complement is an ordinary flag.
    /// assert!(enabled.apply(UiState::HOVER, true).contains(UiState::HOVER));
    /// ```
    pub const fn apply(self, state: Self, on: bool) -> Self {
        let mut bits = if on {
            self.0 | state.0
        } else {
            self.0 & !state.0
        };
        let mut index = 0;
        while index < COMPLEMENTS.len() {
            let (positive, negative) = COMPLEMENTS[index];
            if state.0 & positive.0 != 0 {
                bits = if on {
                    bits & !negative.0
                } else {
                    bits | negative.0
                };
            }
            if state.0 & negative.0 != 0 {
                bits = if on {
                    bits & !positive.0
                } else {
                    bits | positive.0
                };
            }
            index += 1;
        }
        Self(bits)
    }

    /// Whether no complementary pair holds both of its halves at once.
    ///
    /// Holding neither half is ordinary — an element that is not a form control is neither
    /// `:valid` nor `:invalid` — so only the contradiction is reported. A set built through
    /// [`UiState::apply`] always satisfies this; one built by combining bits directly need not,
    /// which is what the check is for.
    ///
    /// ```
    /// use zgui_vocab::UiState;
    ///
    /// assert!(UiState::EMPTY.pairs_are_consistent());
    /// assert!(!(UiState::ENABLED | UiState::DISABLED).pairs_are_consistent());
    /// ```
    pub const fn pairs_are_consistent(self) -> bool {
        let mut index = 0;
        while index < COMPLEMENTS.len() {
            let (positive, negative) = COMPLEMENTS[index];
            let held = (self.0 & positive.0 != 0) as u8 + (self.0 & negative.0 != 0) as u8;
            if held == 2 {
                return false;
            }
            index += 1;
        }
        true
    }
}
