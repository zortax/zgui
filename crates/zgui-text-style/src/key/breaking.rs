//! The breaking key: everything a line-breaking pass depends on, given a shaping key.

use crate::key::digest::Digest;
use crate::style::paragraph::ParagraphStyle;
use crate::style::text::TextStyle;

/// Identifies everything that decides *where the lines fall*, given an already shaped paragraph.
///
/// A shaped paragraph can be broken and aligned many times without touching the shaper, and a
/// layout engine probes a text leaf repeatedly while it resolves a flex or grid track — so each of
/// those probes must cost a break rather than a shape. This key is what makes that checkable: if it
/// moves, a break is owed; if it does not, the previous break still stands.
///
/// It covers the run properties that only affect breaking, the paragraph's alignment and indent,
/// the width being proposed, and the sizes of any atomic inlines the breaker has to pack — those
/// last because an inline box that resized invalidates the break without changing a single glyph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BreakingKey(pub u64);

impl BreakingKey {
    /// The key of one run's style on its own.
    pub fn of(style: &TextStyle) -> Self {
        let mut digest = Digest::new();
        style.hash_breaking(&mut digest);
        Self(digest.finish())
    }

    /// The key of a paragraph's own breaking properties on their own.
    ///
    /// The base direction is *not* among them, because it changes the glyphs; it is part of the
    /// shaping key instead.
    pub fn of_paragraph(style: &ParagraphStyle) -> Self {
        let mut digest = Digest::new();
        style.hash_breaking(&mut digest);
        Self(digest.finish())
    }
}
