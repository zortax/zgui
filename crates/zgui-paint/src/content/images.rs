//! Replaced content into atlas tiles and external textures.
//!
//! A replaced node names its content and says nothing about it; what that name resolves to is
//! decided here. There are exactly two answers, because there are exactly two places pixels can
//! be: inside this framework's own atlas, which is where a decoded still image goes, and inside
//! somebody else's texture, which is where a video frame or another process's surface stays.
//!
//! # Decoding is not done here, and cannot be
//!
//! What arrives is already-decoded, already-premultiplied texels. Decoding a file format is a
//! decision about which codecs an application links, and it happens off the frame thread; this
//! module is reached while a frame is being built and only ever moves bytes into a tile.

use zgui_atlas::{Atlas, AtlasKey, TextureKind};
use zgui_dom::host::ReplacedId;
use zgui_geom::{Device, Size};
use zgui_scene::ExternalTextureId;

use crate::emit::replaced::Source;

/// What went wrong attaching content to a replaced node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ImageError {
    /// The byte count does not match the extent: four premultiplied bytes per texel are needed.
    #[error("an image of {size:?} needs {expected} bytes and {actual} were given")]
    WrongByteCount {
        /// The extent that was declared.
        size: Size<u32, Device>,
        /// How many bytes that extent needs.
        expected: usize,
        /// How many were given.
        actual: usize,
    },
}

/// One piece of replaced content, as the frame draws it.
#[derive(Clone, Debug)]
pub(crate) enum Content {
    /// Texels this framework owns, cached in its own atlas under `key`.
    Decoded {
        /// What the atlas caches the texels under.
        key: AtlasKey,
        /// The extent of the image.
        size: Size<u32, Device>,
        /// The texels, premultiplied and tightly packed, held so the tile can be rebuilt after an
        /// eviction or a lost device without asking the application to decode again.
        texels: std::sync::Arc<Vec<u8>>,
    },
    /// A texture somebody else owns, drawn from where it already is.
    External(ExternalTextureId),
}

impl Content {
    /// Builds a decoded entry, checking the bytes against the extent.
    pub(crate) fn decoded(
        id: ReplacedId,
        size: Size<u32, Device>,
        texels: Vec<u8>,
    ) -> Result<Self, ImageError> {
        let expected = size.width as usize * size.height as usize * 4;
        if texels.len() != expected {
            return Err(ImageError::WrongByteCount {
                size,
                expected,
                actual: texels.len(),
            });
        }
        Ok(Self::Decoded {
            key: AtlasKey::new(handle(id), TextureKind::Color),
            size,
            texels: std::sync::Arc::new(texels),
        })
    }

    /// How many bytes of decoded texels this entry is holding outside the atlas.
    ///
    /// Zero for external content, which is somebody else's memory and would be counted twice by a
    /// budget that claimed it.
    pub(crate) fn held_bytes(&self) -> u64 {
        match self {
            Self::External(_) => 0,
            Self::Decoded { texels, .. } => texels.len() as u64,
        }
    }
}

/// Where one piece of content is right now, putting it in the atlas if it is not there yet.
///
/// The key comes back with the source, and is `None` for content this framework does not own: an
/// external texture is somebody else's to free, so there is nothing here to hold it by. A caller
/// that is going to replay the primitive rather than emit it again needs the key — see
/// [`hold`](crate::walk::replay::hold) — and cannot recover it from the tile.
pub(crate) fn source_of(
    atlas: &mut Atlas,
    content: &Content,
) -> Option<(Source, Option<AtlasKey>)> {
    match content {
        Content::External(texture) => Some((Source::External(*texture), None)),
        Content::Decoded { key, size, texels } => {
            let extent = Size::new(size.width as i32, size.height as i32);
            let texels = std::sync::Arc::clone(texels);
            let tile = atlas
                .get_or_insert(*key, extent, || texels.as_ref().clone())
                .ok()?;
            Some((Source::Decoded(tile.into()), Some(*key)))
        }
    }
}

/// The atlas handle one replaced node's content is cached under.
///
/// A node's own generation-checked name, which is already unique for the life of the document and
/// is reissued to nobody: content attached to a node that has been destroyed cannot be served to
/// whatever takes the slot over.
fn handle(id: ReplacedId) -> u64 {
    let key = id.node();
    u64::from(key.index()) << 32 | u64::from(key.generation().get())
}

#[cfg(test)]
mod tests {
    use zgui_geom::Size;

    use super::{Content, ImageError};

    /// A replaced identifier for a node of the first domain.
    fn id() -> zgui_dom::host::ReplacedId {
        use zgui_arena::{DomainId, Generation};
        zgui_dom::host::ReplacedId::new(zgui_dom::NodeKey::new(
            3,
            Generation::FIRST,
            DomainId::FIRST,
        ))
    }

    #[test]
    fn an_image_whose_bytes_do_not_match_its_extent_is_refused() {
        let refused = Content::decoded(id(), Size::new(2, 2), vec![0; 8]);
        assert_eq!(
            refused.unwrap_err(),
            ImageError::WrongByteCount {
                size: Size::new(2, 2),
                expected: 16,
                actual: 8,
            },
            "a short buffer would have the upload read past the end of a row"
        );
        assert!(Content::decoded(id(), Size::new(2, 2), vec![0; 16]).is_ok());
    }
}
