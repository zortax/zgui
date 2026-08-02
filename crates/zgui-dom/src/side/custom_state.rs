//! The states an author named, which `:state(name)` matches.
//!
//! The closed set of interaction states covers what a control *is* — checked, disabled, open — and
//! stops there, because every one of those mirrors an accessibility property and a document that
//! invented more would have invented meaning nothing can read. Everything else a component wants
//! to select on is the author's own vocabulary: a step is complete, a row is the drop target, a
//! panel is peeking. Those live here.
//!
//! A set is almost always empty and never large, so it is a short list rather than a map: two
//! names inline, a comparison per name, and no allocation for the elements that carry one or two.

use smallvec::SmallVec;
use style::values::AtomIdent;

/// The author-defined states one element carries.
#[derive(Clone, Default, Debug)]
pub struct CustomStates {
    /// In the order they were first set, which nothing depends on and which keeps a set stable
    /// enough to read in a test.
    names: SmallVec<[AtomIdent; 2]>,
}

impl CustomStates {
    /// A set with nothing in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many states are set.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether no state is set.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Whether `name` is set.
    pub fn contains(&self, name: &AtomIdent) -> bool {
        self.names.iter().any(|held| held == name)
    }

    /// Turns `name` on or off, and says whether that changed anything.
    pub fn set(&mut self, name: &AtomIdent, on: bool) -> bool {
        match (self.names.iter().position(|held| held == name), on) {
            (Some(_), true) | (None, false) => false,
            (Some(position), false) => {
                self.names.remove(position);
                true
            }
            (None, true) => {
                self.names.push(name.clone());
                true
            }
        }
    }

    /// Every state that is set.
    pub fn iter(&self) -> impl Iterator<Item = &AtomIdent> {
        self.names.iter()
    }
}

#[cfg(test)]
mod tests {
    use style::values::AtomIdent;

    use super::CustomStates;

    #[test]
    fn a_state_goes_on_once_and_off_once() {
        let mut states = CustomStates::new();
        let peeking = AtomIdent::from("peeking");
        assert!(states.is_empty());

        assert!(states.set(&peeking, true));
        assert!(!states.set(&peeking, true), "it was already on");
        assert!(states.contains(&peeking));
        assert_eq!(states.len(), 1);

        assert!(states.set(&peeking, false));
        assert!(!states.set(&peeking, false), "it was already off");
        assert!(!states.contains(&peeking));
        assert!(states.is_empty());
    }

    #[test]
    fn two_states_are_independent() {
        let mut states = CustomStates::new();
        let (a, b) = (AtomIdent::from("a"), AtomIdent::from("b"));
        states.set(&a, true);
        states.set(&b, true);
        states.set(&a, false);
        assert!(!states.contains(&a));
        assert!(states.contains(&b));
        assert_eq!(states.iter().count(), 1);
    }
}
