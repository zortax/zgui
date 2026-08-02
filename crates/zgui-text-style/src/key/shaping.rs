//! The shaping key: everything a shaping pass depends on.

use crate::key::digest::Digest;
use crate::style::paragraph::ParagraphStyle;
use crate::style::text::TextStyle;

/// Identifies everything that decides *which glyphs exist and how wide they are*.
///
/// Two runs with the same shaping key produce byte-identical shaper output for the same text at the
/// same device scale, so a change that leaves this key alone can never require a fresh shape. That
/// is the expensive half of text layout, and keeping it out of the way of everything else is the
/// reason the key is split in two at all — see [`BreakingKey`](crate::BreakingKey).
///
/// What is deliberately *not* in it: the colour. A run's paint is an index into a table the shaped
/// result does not own, so switching theme re-colours cached paragraphs instead of re-shaping them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShapingKey(pub u64);

impl ShapingKey {
    /// The key of one run's style on its own.
    ///
    /// A whole paragraph's key additionally covers its text, its runs' extents and the device
    /// scale, because all three change what the shaper produces; this is the per-run component
    /// such a key is folded from.
    pub fn of(style: &TextStyle) -> Self {
        let mut digest = Digest::new();
        style.hash_shaping(&mut digest);
        Self(digest.finish())
    }

    /// The key of a paragraph's own shaping properties on their own, which is its base direction.
    pub fn of_paragraph(style: &ParagraphStyle) -> Self {
        let mut digest = Digest::new();
        style.hash_shaping(&mut digest);
        Self(digest.finish())
    }
}
