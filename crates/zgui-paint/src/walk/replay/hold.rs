//! What a cached range owns, and the atlas it owns it in.
//!
//! # Why a range has to own anything at all
//!
//! A cached range is replayed by copying last frame's operations forward. Nothing on that path
//! looks a glyph up: the operations already carry the rectangle of the texture the pixels are in,
//! baked into the instance when the range was first encoded. So to the cache holding those pixels,
//! a glyph drawn by a replay is a glyph this frame did not draw — indistinguishable from one that
//! left the screen a hundred frames ago.
//!
//! A cache that frees its coldest content therefore frees exactly the tiles a static label is
//! replaying, hands their rectangles to whatever is rasterised next, and the label goes on drawing
//! the rectangles: not a blank glyph, but whichever glyph occupied the rectangle afterwards. The
//! defect is silent in the display list, which still says what it always said, and silent in the
//! geometry, which never moved.
//!
//! The record closes it by being an *owner*. It is told the keys its range names while the
//! placement that names them is happening — that is [`ResourceOwner::take_named`] — and it holds
//! them for exactly as long as the range stands, which is what makes "this frame did not look it
//! up" stop being the same statement as "nothing is drawing it".
//!
//! # Why the names have to be collected rather than derived
//!
//! There is no way back from a primitive to the key its pixels are cached under. An instance
//! carries a texture index, an allocation handle and a rectangle, because that is what a shader
//! samples with; the key is the atlas's own name for the content, and it is not in the instance
//! and could not usefully be put there. So the keys are collected at the one moment both are in
//! hand, which is the placement.

use zgui_atlas::AtlasKey;

/// The cache a recorded range's tiles live in, as the record is allowed to touch it.
///
/// Four questions, and the walk asks all four: what did the encoding just name, hold this, let go
/// of this, and — for the assertion that catches the defect this trait exists for — is this still
/// there.
pub trait ResourceOwner {
    /// Moves the keys named since the last call onto the end of `out`.
    ///
    /// Draining rather than reading, because the list is the record of *one* encoding: a second
    /// fragment that read the same list without emptying it would take ownership of the first
    /// fragment's glyphs as well, and would go on holding them long after the fragment that drew
    /// them had gone.
    fn take_named(&self, out: &mut Vec<AtlasKey>);

    /// Holds `key` against eviction until it is released.
    fn retain(&self, key: AtlasKey);

    /// Gives up one hold on `key`.
    fn release(&self, key: AtlasKey);

    /// Whether `key`'s content is still cached, without marking it as used.
    ///
    /// Without marking, because this is what an assertion asks: a check that marked would make the
    /// thing it was checking true.
    fn contains(&self, key: AtlasKey) -> bool;
}

/// An owner with nothing to own.
///
/// What a document with no text or pictures paints through, and what a test that is not about
/// content uses so that it does not have to pretend to have an atlas. Its [`take_named`] yields
/// nothing, so records made through it name no resources and there is nothing for them to hold.
///
/// [`take_named`]: ResourceOwner::take_named
#[derive(Clone, Copy, Debug, Default)]
pub struct NoResources;

impl ResourceOwner for NoResources {
    fn take_named(&self, _out: &mut Vec<AtlasKey>) {}

    fn retain(&self, _key: AtlasKey) {}

    fn release(&self, _key: AtlasKey) {}

    /// Nothing is cached, which is the truthful answer and not a permissive one: a record made
    /// through this owner names no keys, so nothing ever asks.
    fn contains(&self, _key: AtlasKey) -> bool {
        false
    }
}
