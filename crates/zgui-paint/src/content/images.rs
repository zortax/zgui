//! Replaced content as decoded texels and external textures.
//!
//! A replaced node names its content and says nothing about it. What that name resolves to is
//! decided here, and there are two answers: texels this framework holds, which the compositor
//! keeps as an image, and somebody else's texture, which is where a video frame or another
//! process's surface stays.
//!
//! # Decoding is not done here
//!
//! What arrives is already-decoded, already-premultiplied texels. Decoding a file format is a
//! decision about which codecs an application links, and it happens off the frame thread.

use zgui_dom::host::ReplacedId;
use zgui_geom::{Device, Size};

use crate::emit::record::ImageKey;

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

/// The handle namespace of images a loader shares between nodes.
///
/// Set on a loader's counter and never on a node's packed name, so the two vocabularies cannot
/// meet and serve one node's pixels for another's.
const SHARED: u64 = 1 << 63;

/// The key a loader's shared handle resolves to.
pub(crate) fn shared_key(handle: u64) -> ImageKey {
    ImageKey(SHARED | handle)
}

/// One piece of replaced content, as the frame draws it.
#[derive(Clone, Debug)]
pub(crate) enum Content {
    /// Texels this framework owns, which the compositor keeps as the image `key`.
    Decoded {
        /// What the texels are kept under.
        key: ImageKey,
        /// The extent of the texels.
        size: Size<u32, Device>,
        /// The image's own extent, which the texels may be a downscale of.
        natural: Size<u32, Device>,
        /// The texels, premultiplied and tightly packed, held so the image can be made again
        /// without asking the application to decode again.
        texels: std::sync::Arc<Vec<u8>>,
    },
    /// Texels the compositor was handed and whose host copy has been given back.
    ///
    /// The state a shared attachment settles into once its image is in the compositor. A frame
    /// that finds the compositor holds no image of it reports the node as missing rather than
    /// drawing, and whoever owns the source decodes it again.
    Uploaded {
        /// What the texels were kept under.
        key: ImageKey,
        /// The image's own extent, which its texels may be a downscale of.
        natural: Size<u32, Device>,
    },
    /// A texture somebody else owns, drawn from where it already is.
    External(iceblink::TextureId),
}

impl Content {
    /// Builds a decoded entry, checking the bytes against the extent.
    pub(crate) fn decoded(
        id: ReplacedId,
        size: Size<u32, Device>,
        texels: Vec<u8>,
    ) -> Result<Self, ImageError> {
        Self::under_key(
            ImageKey(handle(id)),
            size,
            size,
            std::sync::Arc::new(texels),
        )
    }

    /// Builds a decoded entry under a caller-chosen handle, so nodes can share one image.
    ///
    /// The handle is namespaced away from the per-node ones: a loader's counter and a node's
    /// packed name must never meet, because one node's pixels would then be served for the
    /// other's.
    pub(crate) fn shared(
        handle: u64,
        size: Size<u32, Device>,
        natural: Size<u32, Device>,
        texels: std::sync::Arc<Vec<u8>>,
    ) -> Result<Self, ImageError> {
        Self::under_key(shared_key(handle), size, natural, texels)
    }

    /// Builds an entry for a shared image the compositor holds, carrying no texels at all.
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
            Self::Decoded { key, .. } | Self::Uploaded { key, .. } => key.0 & SHARED != 0,
            Self::External(_) => false,
        }
    }

    /// The entry both constructors build, once the bytes are checked against the extent.
    fn under_key(
        key: ImageKey,
        size: Size<u32, Device>,
        natural: Size<u32, Device>,
        texels: std::sync::Arc<Vec<u8>>,
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
            key,
            size,
            natural,
            texels,
        })
    }

    /// How many bytes of decoded texels this entry is holding.
    ///
    /// Zero for external content, which is somebody else's memory and would be counted twice by a
    /// budget that claimed it.
    pub(crate) fn held_bytes(&self) -> u64 {
        match self {
            Self::External(_) | Self::Uploaded { .. } => 0,
            Self::Decoded { texels, .. } => texels.len() as u64,
        }
    }
}

/// The handle one replaced node's content is kept under.
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
