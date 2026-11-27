//! Hashing a side-table entry by what it *is*.
//!
//! The three interned tables key their entries on content, and their entries hold floating-point
//! numbers — colour channels, gradient angles, corner radii. `Hash` and `Eq` are unavailable for
//! those, so this module defines the pair the tables actually need: an exact bit-pattern hash, and
//! ordinary structural equality to settle collisions.
//!
//! Hashing bit patterns rather than values means `0.0` and `-0.0` are different content, and two
//! `NaN`s with the same payload are the same content. Both are correct here: the question a table
//! asks is "have I already stored exactly these bytes", not "are these numerically equal".

/// A value a side table can intern.
///
/// Equality decides whether two entries are the same, and the hash only narrows the search — so an
/// implementation may hash conservatively, but two equal values must hash the same.
pub trait Content: Clone + PartialEq {
    /// A hash of everything equality looks at.
    fn content_hash(&self) -> u64;

    /// Whether replacing a table entry with `other` changes the value a reader observes.
    ///
    /// Usually this is the same question as equality. A value whose stable interned identity is
    /// deliberately narrower than what it stores can override it: a clip node, for example, keeps
    /// one id while scrolling rewrites where its rectangle is drawn.
    fn same_stored_value(&self, other: &Self) -> bool {
        self == other
    }
}

/// An incremental hash over the raw bytes of a value's fields.
///
/// It is FNV-1a: two multiplications and an exclusive-or per byte, no state beyond a `u64`, and
/// good enough for a table whose collisions are settled by comparison anyway. It is here rather
/// than `DefaultHasher` because a stored content hash is compared *across frames*, and a hash whose
/// seed changes per process could not be.
#[derive(Clone, Copy, Debug)]
pub struct ContentHash(u64);

impl Default for ContentHash {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentHash {
    /// FNV-1a's 64-bit offset basis.
    const BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    /// FNV-1a's 64-bit prime.
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    /// A hash of nothing yet.
    pub const fn new() -> Self {
        Self(Self::BASIS)
    }

    /// Folds in every byte of `bytes`.
    pub const fn bytes(mut self, bytes: &[u8]) -> Self {
        let mut index = 0;
        while index < bytes.len() {
            self.0 ^= bytes[index] as u64;
            self.0 = self.0.wrapping_mul(Self::PRIME);
            index += 1;
        }
        self
    }

    /// Folds in a `u64`.
    pub const fn u64(self, value: u64) -> Self {
        self.bytes(&value.to_le_bytes())
    }

    /// Folds in a `u32`.
    pub const fn u32(self, value: u32) -> Self {
        self.u64(value as u64)
    }

    /// Folds in an `i32`.
    pub const fn i32(self, value: i32) -> Self {
        self.u64(value as u32 as u64)
    }

    /// Folds in an `f32` by its bit pattern.
    pub const fn f32(self, value: f32) -> Self {
        self.u32(value.to_bits())
    }

    /// Folds in every element of a slice of `f32`, by bit pattern.
    pub fn f32s(mut self, values: &[f32]) -> Self {
        for value in values {
            self = self.f32(*value);
        }
        self
    }

    /// The hash so far.
    pub const fn finish(self) -> u64 {
        self.0
    }
}

/// A matrix is content: sixteen numbers, hashed as their bit patterns.
///
/// It is here rather than beside the coordinate systems that hold matrices because nothing about a
/// coordinate system is interned — what asks for this is a record that kept a fingerprint of the
/// matrix it drew through, so that a matrix which moved is re-encoded rather than replayed.
impl Content for zgui_geom::Matrix4 {
    fn content_hash(&self) -> u64 {
        let mut hash = ContentHash::new();
        for column in self.columns {
            hash = hash.f32s(&column);
        }
        hash.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::ContentHash;

    #[test]
    fn the_same_bytes_hash_the_same_every_time() {
        let once = ContentHash::new().f32(1.5).u32(7).finish();
        let again = ContentHash::new().f32(1.5).u32(7).finish();
        assert_eq!(once, again);
    }

    #[test]
    fn field_order_is_part_of_the_content() {
        let forwards = ContentHash::new().u32(1).u32(2).finish();
        let backwards = ContentHash::new().u32(2).u32(1).finish();
        assert_ne!(forwards, backwards);
    }

    #[test]
    fn signed_zeroes_are_different_content() {
        assert_ne!(
            ContentHash::new().f32(0.0).finish(),
            ContentHash::new().f32(-0.0).finish()
        );
    }
}
