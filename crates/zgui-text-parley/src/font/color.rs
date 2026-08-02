//! Whether a face draws in colour, and which mechanism it draws with.

use skrifa::{FontRef, MetadataProvider, Tag};

/// The colour mechanisms a face can carry.
///
/// Probed from the face's own tables rather than guessed from its name. A name match is wrong in
/// both directions: a face called `Noto Color Emoji` on a system that shipped the monochrome build
/// has no colour glyphs, and a musical-notation or icon face nobody would call an emoji font
/// frequently has them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ColorSupport {
    /// The face carries layered colour outlines — `COLR` with `CPAL`.
    pub outlines: bool,
    /// The face carries colour bitmap strikes — `CBDT`/`CBLC` or `sbix`.
    pub bitmaps: bool,
}

impl ColorSupport {
    /// Nothing in colour, which is what a text face reports.
    pub const NONE: Self = Self {
        outlines: false,
        bitmaps: false,
    };

    /// Whether the face draws any glyph in colour at all.
    pub fn any(self) -> bool {
        self.outlines || self.bitmaps
    }

    /// Reads the two mechanisms out of one face.
    ///
    /// Bytes that are not a readable face report nothing rather than failing: a face that cannot
    /// be opened has no colour glyphs, which is the same answer the caller needs.
    pub fn probe(data: &[u8], index: u32) -> Self {
        let Ok(font) = FontRef::from_index(data, index) else {
            return Self::NONE;
        };
        Self {
            outlines: font.table_data(Tag::new(b"COLR")).is_some()
                && !font.color_palettes().is_empty(),
            bitmaps: !font.bitmap_strikes().is_empty(),
        }
    }
}
