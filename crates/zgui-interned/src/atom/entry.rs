//! The record one interned string leaves behind for the life of the process.

use core::hash::{BuildHasher, Hash, Hasher};

use rustc_hash::FxBuildHasher;

/// One interned string, together with the hash of its text.
///
/// The hash is stored rather than recomputed because the point of interning is that comparing and
/// hashing a name should not depend on how long the name is. It is a hash of the *text*, not of
/// the entry's address, so a map keyed by interned names iterates in the same order on every run
/// — which is what makes a recorded transcript comparable between runs.
#[derive(Debug)]
pub(crate) struct Entry {
    /// The text, owned for the rest of the process.
    text: Box<str>,
    /// The hash of `text`.
    hash: u64,
}

impl Entry {
    /// Records `text` and its hash.
    pub(crate) fn new(text: &str) -> Self {
        Self {
            text: text.into(),
            hash: hash_of(text),
        }
    }

    /// The interned text.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// The hash of the interned text.
    pub(crate) fn hash(&self) -> u64 {
        self.hash
    }
}

impl Hash for Entry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

/// The hash every interned string is looked up and compared by.
fn hash_of(text: &str) -> u64 {
    FxBuildHasher.hash_one(text)
}
