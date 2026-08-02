//! Properties a caller sets on an element imperatively.
//!
//! These are not attributes and they are deliberately invisible to selector matching. A text
//! field's current value is the worked example: it changes on every keystroke, and if it were an
//! attribute every keystroke would take a snapshot and invalidate every rule that could possibly
//! depend on one. What it actually needs is somewhere to live that the accessibility projection and
//! the editing model can read.

use smallvec::SmallVec;
use zgui_vocab::{PropKey, PropValue};

/// The imperative properties set on one node.
#[derive(Clone, Default, Debug)]
pub struct PropMap {
    /// In insertion order. A node with properties has one or two.
    entries: SmallVec<[(PropKey, PropValue); 2]>,
}

impl PropMap {
    /// A map with nothing in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many properties the map holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the map holds nothing.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The value of `key`, if it is set.
    pub fn get(&self, key: PropKey) -> Option<&PropValue> {
        self.entries
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value)
    }

    /// Sets `key`, replacing any previous value, and returns what was there.
    pub fn set(&mut self, key: PropKey, value: PropValue) -> Option<PropValue> {
        match self.entries.iter_mut().find(|(name, _)| *name == key) {
            Some((_, slot)) => Some(core::mem::replace(slot, value)),
            None => {
                self.entries.push((key, value));
                None
            }
        }
    }

    /// Removes `key`, returning its value if it was set.
    pub fn remove(&mut self, key: PropKey) -> Option<PropValue> {
        let position = self.entries.iter().position(|(name, _)| *name == key)?;
        Some(self.entries.remove(position).1)
    }

    /// Every property, in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &(PropKey, PropValue)> {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use zgui_vocab::{PropKey, PropValue};

    use super::PropMap;

    #[test]
    fn setting_a_property_twice_replaces_it() {
        let mut props = PropMap::new();
        assert!(props.is_empty());
        assert_eq!(
            props.set(PropKey::new("value"), PropValue::Bool(false)),
            None
        );
        assert_eq!(
            props.set(PropKey::new("value"), PropValue::Bool(true)),
            Some(PropValue::Bool(false))
        );
        assert_eq!(props.len(), 1);
        assert_eq!(
            props.get(PropKey::new("value")),
            Some(&PropValue::Bool(true))
        );
    }

    #[test]
    fn removing_a_property_yields_it_once() {
        let mut props = PropMap::new();
        props.set(PropKey::new("value"), PropValue::Bool(true));
        assert_eq!(
            props.remove(PropKey::new("value")),
            Some(PropValue::Bool(true))
        );
        assert_eq!(props.remove(PropKey::new("value")), None);
        assert_eq!(props.iter().count(), 0);
    }
}
