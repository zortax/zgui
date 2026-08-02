//! The name half of an imperative property.

use core::fmt::{self, Debug, Display};

use zgui_interned::Atom;

/// The name of an imperative property.
///
/// Three ways of putting a value on an element look similar and are not. An *attribute* is text
/// that selectors can match and that a stylesheet can therefore react to. A *state* is one of a
/// fixed set of bits that selectors can match. A *property* is neither: it is a typed value the
/// element's own behaviour reads — a field's current text, a scroll container's offset, a media
/// element's volume — and nothing in a stylesheet can see it.
///
/// Keeping properties out of the attribute space is what stops every keystroke in a text field
/// from invalidating selector matching for the whole subtree.
///
/// The name is interned, so comparing two keys is a pointer comparison and a key costs eight
/// bytes to store beside a value.
///
/// ```
/// use zgui_vocab::PropKey;
///
/// let value = PropKey::new("value");
/// assert_eq!(value, PropKey::new("value"));
/// assert_eq!(value.as_str(), "value");
/// ```
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PropKey(Atom);

impl PropKey {
    /// The key for `name`, interning it the first time it is seen.
    ///
    /// Property names come from a vocabulary written into the program, so the set is small and
    /// bounded. Do not build one out of text a user typed.
    pub fn new(name: &str) -> Self {
        Self(Atom::new(name))
    }

    /// The name, borrowed for the lifetime of the program.
    pub fn as_str(self) -> &'static str {
        self.0.as_str()
    }

    /// The interned name behind this key.
    pub fn atom(self) -> Atom {
        self.0
    }
}

impl From<&str> for PropKey {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl PartialEq<str> for PropKey {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl Display for PropKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Debug for PropKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "PropKey({:?})", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::PropKey;

    #[test]
    fn equal_names_intern_to_the_same_key() {
        assert_eq!(PropKey::new("value"), PropKey::from("value"));
        assert_ne!(PropKey::new("value"), PropKey::new("checked"));
    }

    #[test]
    fn a_key_is_one_pointer_wide() {
        assert_eq!(size_of::<PropKey>(), size_of::<usize>());
    }
}
