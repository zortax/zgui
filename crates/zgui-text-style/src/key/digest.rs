//! The hasher both keys are built with.
//!
//! Two properties matter and neither is negotiable. It must be **stable across processes**, because
//! a key that changed between runs would make every cached measurement in a test suite depend on
//! which run took it. And it must treat *equal values* as equal, which for floating point means
//! folding negative zero onto zero — `letter-spacing: -0px` and `letter-spacing: 0px` are the same
//! spacing, and hashing their raw bit patterns would say otherwise.

use std::hash::{Hash, Hasher};

use rustc_hash::FxHasher;
use zgui_geom::CssPx;

/// An accumulating hash over the parts of a style.
///
/// ```
/// use zgui_text_style::Digest;
///
/// let mut one = Digest::new();
/// one.push_f32(0.0);
///
/// let mut other = Digest::new();
/// other.push_f32(-0.0);
///
/// assert_eq!(one.finish(), other.finish(), "the same length hashes the same way");
/// ```
#[derive(Clone, Default)]
pub struct Digest {
    /// The hasher itself.
    hasher: FxHasher,
}

impl Digest {
    /// An empty digest.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mixes in anything hashable, which covers every enumerated value.
    pub fn push(&mut self, value: impl Hash) {
        value.hash(&mut self.hasher);
    }

    /// Mixes in a discriminant, so that two differently shaped values cannot collide by holding
    /// the same payload.
    pub fn push_tag(&mut self, tag: u8) {
        self.hasher.write_u8(tag);
    }

    /// Mixes in a number, folding negative zero onto zero.
    pub fn push_f32(&mut self, value: f32) {
        let normalised = if value == 0.0 { 0.0 } else { value };
        self.hasher.write_u32(normalised.to_bits());
    }

    /// Mixes in a length.
    pub fn push_length(&mut self, value: CssPx) {
        self.push_f32(value.0);
    }

    /// Mixes in an optional length, distinguishing "absent" from any value it could hold.
    pub fn push_optional_length(&mut self, value: Option<CssPx>) {
        match value {
            None => self.push_tag(0),
            Some(length) => {
                self.push_tag(1);
                self.push_length(length);
            }
        }
    }

    /// The accumulated hash.
    pub fn finish(&self) -> u64 {
        self.hasher.finish()
    }
}

impl core::fmt::Debug for Digest {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Digest")
            .field("hash", &self.finish())
            .finish()
    }
}
