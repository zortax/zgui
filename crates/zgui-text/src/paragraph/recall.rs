//! What a shaped paragraph remembers about the widths it has already been broken at.

use smallvec::SmallVec;
use zgui_text_style::BreakingKey;

use crate::paragraph::broken::BrokenParagraph;

/// How many breaking results one paragraph keeps.
///
/// A layout algorithm asks a paragraph the same three questions in a row — how narrow it can be,
/// how wide it wants to be, and how tall it is at the width it has been given — and then asks them
/// again on the next iteration of whatever it is resolving. Three is therefore the number that
/// makes the second round free; the fourth slot is there because a nested grid adds a candidate
/// width of its own, and a fifth would be paying for a case that has not been observed.
const REMEMBERED: usize = 4;

/// The breaking results one shaped paragraph is holding, most recent last.
///
/// Bounded on purpose. The alternative is a map that grows with the number of distinct widths a
/// document has ever proposed, which for a window being dragged is a new entry every frame for as
/// long as the drag lasts, for every paragraph on the page.
#[derive(Clone, Debug, Default)]
pub(crate) struct Recalled {
    /// The results, in the order they were taken.
    entries: SmallVec<[(BreakingKey, BrokenParagraph); REMEMBERED]>,
}

impl Recalled {
    /// The result held for one key, if it is still held.
    pub(crate) fn get(&self, key: BreakingKey) -> Option<&BrokenParagraph> {
        self.entries
            .iter()
            .find(|(held, _)| *held == key)
            .map(|(_, broken)| broken)
    }

    /// Holds one result, replacing the oldest if there is no room.
    pub(crate) fn insert(&mut self, key: BreakingKey, broken: BrokenParagraph) {
        if let Some(slot) = self.entries.iter_mut().find(|(held, _)| *held == key) {
            slot.1 = broken;
            return;
        }
        if self.entries.len() == REMEMBERED {
            self.entries.remove(0);
        }
        self.entries.push((key, broken));
    }

    /// How many results are held.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}
