//! Faces, as the rest of the pipeline refers to them.

use zgui_interned::Ident;
use zgui_text_style::FontSlant;

/// A handle to one face held by a [`FontSource`](crate::FontSource).
///
/// Opaque and cheap: it is what a shaped run stores, what a glyph raster key is built from, and
/// what an atlas entry is attributed to. The number means nothing outside the source that issued
/// it, and two sources' handles must never be mixed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FaceId(pub u32);

/// What a source knows about one face without opening it.
///
/// The three axis values are the face's *own* position, not the position that was asked for: a
/// request for weight 500 that matched a face at 400 reports 400 here, which is what tells a
/// consumer that synthetic emboldening is needed.
#[derive(Clone, Debug, PartialEq)]
pub struct FaceRecord {
    /// The handle.
    pub id: FaceId,
    /// The family the face belongs to.
    pub family: Ident,
    /// The face's own weight.
    pub weight: f32,
    /// The face's own slant.
    pub slant: FontSlant,
    /// The face's own width, as a fraction of normal.
    pub width: f32,
    /// Whether the face has variable axes, and so can be instanced rather than only selected.
    pub is_variable: bool,
    /// Whether the face carries colour glyphs, which decides which rasterisation path a run takes.
    pub has_color: bool,
}
