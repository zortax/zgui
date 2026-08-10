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

use zgui_atlas::{Atlas, AtlasError, AtlasKey, TextureKind};
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

/// One level of detail below an image's own resolution.
#[derive(Clone, Debug)]
pub struct MipLevel {
    /// The level's extent: half the previous one's, rounded down, never below one.
    pub size: Size<u32, Device>,
    /// The level's texels, in the same format as the image's own.
    pub texels: std::sync::Arc<Vec<u8>>,
}

/// The handle namespace of tiles a loader shares between nodes.
///
/// Set on a loader's counter and never on a node's packed name, so the two vocabularies cannot
/// meet in the atlas and serve one node's pixels for another's.
const SHARED: u64 = 1 << 63;

/// The atlas key a loader's shared handle resolves to.
pub(crate) fn shared_key(handle: u64) -> AtlasKey {
    AtlasKey::new(SHARED | handle, TextureKind::Image)
}

/// One piece of replaced content, as the frame draws it.
#[derive(Clone, Debug)]
pub(crate) enum Content {
    /// Texels this framework owns, cached in its own atlas under `key`.
    Decoded {
        /// What the atlas caches the texels under.
        key: AtlasKey,
        /// The extent of the texels.
        size: Size<u32, Device>,
        /// The image's own extent, which the texels may be a downscale of.
        natural: Size<u32, Device>,
        /// The texels, premultiplied and tightly packed, held so the tile can be rebuilt after an
        /// eviction or a lost device without asking the application to decode again.
        texels: std::sync::Arc<Vec<u8>>,
        /// Levels of detail below `texels`, for an image large enough to have its own texture.
        ///
        /// Empty for content that shares an atlas page: a page holds many tiles and can have no
        /// level of detail of its own. An image that arrives with levels is cached standalone —
        /// the texture is the tile — and minified sampling reads them.
        mips: Vec<MipLevel>,
    },
    /// Texels whose tile is resident and whose host copy has been given back.
    ///
    /// The state a shared attachment settles into once its upload has flushed: the atlas serves
    /// the tile, and nothing on the host doubles it. A frame that finds the tile gone — evicted,
    /// or lost with the device — reports the node as missing rather than drawing, and whoever
    /// owns the source decodes it again.
    Uploaded {
        /// What the atlas caches the texels under; the tile itself carries the extent.
        key: AtlasKey,
        /// The image's own extent, which the tile's texels may be a downscale of.
        natural: Size<u32, Device>,
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
        Self::under_key(
            AtlasKey::new(handle(id), TextureKind::Image),
            size,
            size,
            std::sync::Arc::new(texels),
            Vec::new(),
        )
    }

    /// Builds a decoded entry under a caller-chosen handle, so nodes can share one tile.
    ///
    /// The handle is namespaced away from the per-node ones: a loader's counter and a node's
    /// packed name must never meet in the atlas, because the atlas would then serve one node's
    /// pixels for the other's.
    pub(crate) fn shared(
        handle: u64,
        size: Size<u32, Device>,
        natural: Size<u32, Device>,
        texels: std::sync::Arc<Vec<u8>>,
        mips: Vec<MipLevel>,
    ) -> Result<Self, ImageError> {
        Self::under_key(shared_key(handle), size, natural, texels, mips)
    }

    /// Builds an entry for a shared tile that is already resident, carrying no texels at all.
    pub(crate) fn uploaded_shared(handle: u64, natural: Size<u32, Device>) -> Self {
        Self::Uploaded {
            key: shared_key(handle),
            natural,
        }
    }

    /// Whether this entry's key is in the loader-shared namespace.
    ///
    /// A shared entry has an owner that can decode it again, which is what makes dropping its
    /// host texels safe; a per-node entry was attached directly and its texels are the only copy.
    pub(crate) fn is_shared(&self) -> bool {
        match self {
            Self::Decoded { key, .. } | Self::Uploaded { key, .. } => key.handle() & SHARED != 0,
            Self::External(_) => false,
        }
    }

    /// The entry both constructors build, once the bytes are checked against the extent.
    fn under_key(
        key: AtlasKey,
        size: Size<u32, Device>,
        natural: Size<u32, Device>,
        texels: std::sync::Arc<Vec<u8>>,
        mips: Vec<MipLevel>,
    ) -> Result<Self, ImageError> {
        let expected = size.width as usize * size.height as usize * 4;
        if texels.len() != expected {
            return Err(ImageError::WrongByteCount {
                size,
                expected,
                actual: texels.len(),
            });
        }
        for level in &mips {
            let expected = level.size.width as usize * level.size.height as usize * 4;
            if level.texels.len() != expected {
                return Err(ImageError::WrongByteCount {
                    size: level.size,
                    expected,
                    actual: level.texels.len(),
                });
            }
        }
        Ok(Self::Decoded {
            key,
            size,
            natural,
            texels,
            mips,
        })
    }

    /// How many bytes of decoded texels this entry is holding outside the atlas.
    ///
    /// Zero for external content, which is somebody else's memory and would be counted twice by a
    /// budget that claimed it.
    pub(crate) fn held_bytes(&self) -> u64 {
        match self {
            Self::External(_) | Self::Uploaded { .. } => 0,
            Self::Decoded { texels, mips, .. } => {
                texels.len() as u64
                    + mips
                        .iter()
                        .map(|level| level.texels.len() as u64)
                        .sum::<u64>()
            }
        }
    }
}

/// Where one piece of content is right now, putting it in the atlas if it is not there yet.
///
/// The key comes back with the source, and is `None` for content this framework does not own: an
/// external texture is somebody else's to free, so there is nothing here to hold it by. A caller
/// that is going to replay the primitive rather than emit it again needs the key — see
/// [`hold`](crate::walk::replay::hold) — and cannot recover it from the tile.
///
/// `Ok(None)` is an [`Uploaded`](Content::Uploaded) entry whose tile is gone: there are no texels
/// here to rebuild it from, so the node draws nothing this frame and the caller reports it for a
/// re-decode.
///
/// # Errors
///
/// Whatever the atlas refused the insertion with. [`AtlasError::OutOfSpace`] is a signal to evict
/// and call again; the rest are final for this content.
pub(crate) fn source_of(
    atlas: &mut Atlas,
    content: &Content,
) -> Result<Option<(Source, Option<AtlasKey>)>, AtlasError> {
    match content {
        Content::External(texture) => Ok(Some((Source::External(*texture), None))),
        Content::Uploaded { key, natural, .. } => Ok(atlas.get(*key).map(|tile| {
            (
                Source::Decoded {
                    resource: tile.into(),
                    natural: *natural,
                },
                Some(*key),
            )
        })),
        Content::Decoded {
            key,
            size,
            natural,
            texels,
            mips,
        } => {
            let extent = Size::new(size.width as i32, size.height as i32);
            let tile = if mips.is_empty() {
                atlas.get_or_insert(*key, extent, || std::sync::Arc::clone(texels))?
            } else {
                atlas.insert_standalone(*key, extent, || {
                    std::iter::once(std::sync::Arc::clone(texels))
                        .chain(
                            mips.iter()
                                .map(|level| std::sync::Arc::clone(&level.texels)),
                        )
                        .collect::<Vec<_>>()
                })?
            };
            Ok(Some((
                Source::Decoded {
                    resource: tile.into(),
                    natural: *natural,
                },
                Some(*key),
            )))
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
