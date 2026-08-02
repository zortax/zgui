//! The pointer-sized handle every interned name is built on.

mod entry;
mod pool;

use core::cmp::Ordering;
use core::fmt;
use core::hash::{Hash, Hasher};

use crate::atom::entry::Entry;

/// A string that has been interned: one shared copy, and a handle the size of a pointer.
///
/// Two atoms holding the same text are the same handle, so comparing them is a pointer
/// comparison whatever the length of the text. Cloning one copies eight bytes and touches no
/// allocator, which is why a name can sit in a hot per-node record without costing anything to
/// read or copy.
///
/// Interning is exact: `Atom::new("DIV")` and `Atom::new("div")` are different atoms. Any case
/// folding a caller's vocabulary needs happens before interning, not inside it.
///
/// ```
/// use zgui_interned::Atom;
///
/// let first = Atom::new("border-radius");
/// let second = Atom::new(&String::from("border-radius"));
/// assert_eq!(first, second);
/// assert!(first.is(second));
/// assert_eq!(first.as_str(), "border-radius");
/// assert_eq!(size_of::<Option<Atom>>(), size_of::<usize>());
/// ```
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct Atom(&'static Entry);

impl Atom {
    /// The atom for `text`, interning it if it has not been seen before.
    pub fn new(text: &str) -> Self {
        Self(pool::intern(text))
    }

    /// The interned text.
    pub fn as_str(self) -> &'static str {
        self.0.text()
    }

    /// Whether two atoms are the very same interned string.
    ///
    /// Equal atoms are always identical — that is what interning guarantees — so this exists to
    /// say so explicitly where the identity, rather than the equality, is the point.
    pub fn is(self, other: Self) -> bool {
        core::ptr::eq(self.0, other.0)
    }

    /// Whether the interned text is empty.
    pub fn is_empty(self) -> bool {
        self.as_str().is_empty()
    }

    /// The length of the interned text in bytes.
    pub fn len(self) -> usize {
        self.as_str().len()
    }
}

impl PartialEq for Atom {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.0, other.0)
    }
}

impl Eq for Atom {}

/// Orders atoms by their text, so a sorted list of names is in the order a reader expects and is
/// the same on every run.
impl Ord for Atom {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialOrd for Atom {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Hashes the text, not the address, so that a hash map keyed by atoms iterates identically on
/// every run of the same program.
impl Hash for Atom {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.0.hash());
    }
}

impl Default for Atom {
    fn default() -> Self {
        Self::new("")
    }
}

impl fmt::Debug for Atom {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), formatter)
    }
}

impl fmt::Display for Atom {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl AsRef<str> for Atom {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for Atom {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for Atom {
    fn from(text: String) -> Self {
        Self::new(&text)
    }
}

#[cfg(test)]
mod tests;
