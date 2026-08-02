//! What has focus, what shows a focus ring, and what contains the focused element.
//!
//! Three bits, and they are not the same bit written three ways. `:focus` is the element itself.
//! `:focus-within` is that element and every ancestor of it, which is what lets a field's wrapper
//! draw the ring instead of the field. `:focus-visible` is a judgement about *how* focus arrived:
//! a keyboard user needs to see where they are and a pointer user has just pointed at it, so a
//! ring that appears on every click is noise while a ring that never appears is a keyboard trap
//! you cannot see.

use smallvec::SmallVec;
use zgui_dom::{Document, NodeKey, StyleFilter};
use zgui_vocab::UiState;

use crate::hit::HitChain;
use crate::state::within::move_bit;

/// How focus arrived, which is what decides whether a ring is drawn.
///
/// The rule is stated rather than guessed: a keyboard or a programmatic move shows the ring, and a
/// pointer press does not. A control that wants the ring under a pointer as well — a text field,
/// where the caret is the affordance — asks for it by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FocusSource {
    /// Focus moved because a key was pressed: sequential navigation, or a shortcut.
    Keyboard,
    /// Focus moved because something was pressed with a pointer.
    Pointer,
    /// Focus was moved by the application rather than by the user.
    Script,
}

impl FocusSource {
    /// Whether focus arriving this way should show a ring.
    pub const fn shows_ring(self) -> bool {
        matches!(self, Self::Keyboard | Self::Script)
    }
}

/// Which element has focus, and the writes that keep the three bits true.
#[derive(Clone, Debug, Default)]
pub struct Focus {
    /// The focused element, if any.
    focused: Option<NodeKey>,
    /// The focused element and its ancestors, root first: what carries `:focus-within`.
    within: SmallVec<[NodeKey; 8]>,
    /// Whether the focused element is showing a ring.
    ring: bool,
}

impl Focus {
    /// The focused element.
    pub fn focused(&self) -> Option<NodeKey> {
        self.focused
    }

    /// Whether the focused element is showing a focus ring.
    pub fn shows_ring(&self) -> bool {
        self.ring
    }

    /// The focused element and every ancestor of it, root first.
    pub fn within(&self) -> &[NodeKey] {
        &self.within
    }

    /// Moves focus to the element `chain` ends at, or clears it when the chain is empty.
    ///
    /// Answers with the element that lost focus and the one that gained it, in that order, which
    /// is what the two focus events are dispatched from. Focusing what is already focused answers
    /// with neither and writes nothing — except the ring, which can change without focus moving:
    /// clicking the element the keyboard had already reached takes the ring away.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn move_to(
        &mut self,
        document: &Document,
        filter: &dyn StyleFilter,
        chain: &HitChain,
        source: FocusSource,
    ) -> (Option<NodeKey>, Option<NodeKey>) {
        let target = chain.target();
        let ring = target.is_some() && source.shows_ring();
        if target == self.focused {
            if ring != self.ring {
                self.write_ring(document, filter, ring);
            }
            return (None, None);
        }

        let lost = self.focused;
        let held: SmallVec<[NodeKey; 1]> = self.focused.into_iter().collect();
        let gained: SmallVec<[NodeKey; 1]> = target.into_iter().collect();
        move_bit(document, filter, UiState::FOCUS, &held, &gained);
        move_bit(
            document,
            filter,
            UiState::FOCUS_WITHIN,
            &self.within,
            chain.path(),
        );
        if self.ring {
            move_bit(document, filter, UiState::FOCUS_RING, &held, &[]);
            self.ring = false;
        }

        self.focused = target;
        self.within = chain.path().iter().copied().collect();
        if ring {
            self.write_ring(document, filter, true);
        }
        (lost, target)
    }

    /// Clears focus altogether, which is what pressing the background does.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn clear(
        &mut self,
        document: &Document,
        filter: &dyn StyleFilter,
    ) -> (Option<NodeKey>, Option<NodeKey>) {
        self.move_to(document, filter, &HitChain::default(), FocusSource::Script)
    }

    /// Turns the ring on or off for whatever is focused.
    fn write_ring(&mut self, document: &Document, filter: &dyn StyleFilter, on: bool) {
        let held: SmallVec<[NodeKey; 1]> = self.focused.into_iter().collect();
        let (from, to): (&[NodeKey], &[NodeKey]) = if on { (&[], &held) } else { (&held, &[]) };
        move_bit(document, filter, UiState::FOCUS_RING, from, to);
        self.ring = on;
    }
}
