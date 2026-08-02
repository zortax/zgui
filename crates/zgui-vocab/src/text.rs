//! The immutable string type carried by values that are not names.

use core::borrow::Borrow;
use core::fmt::{self, Debug, Display};
use core::ops::Deref;
use std::sync::Arc;

/// An immutable string whose clone is a refcount bump.
///
/// A name — an element name, a class, a property key — is interned, because the vocabulary of
/// names is small and fixed and comparison is the operation that matters. A *value* is neither:
/// the text in a field, the payload of a clipboard read and the label on a control are
/// attacker-controlled, unbounded and short-lived, and interning them would leak. This is the
/// type for those, and it is deliberately the only one, so nothing in the tree has to decide
/// between three ways of passing a string that is neither borrowed nor uniquely owned.
///
/// ```
/// use zgui_vocab::SharedString;
///
/// let value = SharedString::from("hello");
/// let same = value.clone();
/// assert_eq!(value, same);
/// assert_eq!(&*value, "hello");
/// ```
///
/// It satisfies the cheap-clone string contract, so anything generic over that accepts it:
///
/// ```
/// use zgui_interned::CheapCloneStr;
/// use zgui_vocab::SharedString;
///
/// fn takes_cheap<S: CheapCloneStr>(text: S) -> usize {
///     text.as_ref().len()
/// }
/// assert_eq!(takes_cheap(SharedString::from("abc")), 3);
/// ```
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SharedString(Arc<str>);

impl SharedString {
    /// The empty string, without allocating a new buffer for it each time.
    pub fn empty() -> Self {
        Self::default()
    }

    /// The text, borrowed for as long as this handle lives.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether the string has no characters.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The length in UTF-8 bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl Default for SharedString {
    fn default() -> Self {
        Self(Arc::from(""))
    }
}

impl Deref for SharedString {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for SharedString {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for SharedString {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SharedString {
    fn from(text: &str) -> Self {
        Self(Arc::from(text))
    }
}

impl From<String> for SharedString {
    fn from(text: String) -> Self {
        Self(Arc::from(text))
    }
}

impl From<Arc<str>> for SharedString {
    fn from(text: Arc<str>) -> Self {
        Self(text)
    }
}

impl PartialEq<str> for SharedString {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for SharedString {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl Display for SharedString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&*self.0, formatter)
    }
}

impl Debug for SharedString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&*self.0, formatter)
    }
}

#[cfg(test)]
mod tests {
    use super::SharedString;

    #[test]
    fn clones_share_one_buffer() {
        let first = SharedString::from("shared");
        let second = first.clone();
        assert!(core::ptr::eq(first.as_str(), second.as_str()));
    }

    #[test]
    fn the_empty_string_is_the_default() {
        assert!(SharedString::default().is_empty());
        assert_eq!(SharedString::empty().len(), 0);
    }

    #[test]
    fn compares_against_plain_strings_without_allocating() {
        let value = SharedString::from("abc".to_string());
        assert_eq!(value, "abc");
        assert_eq!(value.to_string(), "abc");
    }
}
