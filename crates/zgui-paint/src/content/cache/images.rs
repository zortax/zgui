//! The image half of a window's content: what is attached to each replaced node.

use core::cell::{Cell, RefCell};

use rustc_hash::{FxHashMap, FxHashSet};
use zgui_dom::host::ReplacedId;
use zgui_geom::{Device, Size};

use crate::content::cache::{ContentCache, FrameContent};
use crate::content::images::{Content, ImageError, shared_key};
use crate::emit::record::ImageKey;
use crate::emit::replaced::Source;
use crate::walk::order::ReplacedSource;

/// What is attached to each replaced node, and what frames found of it.
#[derive(Debug, Default)]
pub(crate) struct ImageContent {
    /// What is attached to each replaced node.
    attached: FxHashMap<ReplacedId, Content>,
    /// How many times each replaced node's attachment has changed.
    ///
    /// Read into the paint record's signature, so a record kept across frames notices new pixels
    /// under an unchanged name. An id that was never attached reads zero.
    revisions: FxHashMap<ReplacedId, u64>,
    /// How many times a frame has resolved a replaced node to the content held for it.
    ///
    /// Monotonic and never reset, so two readings subtracted say whether anything drew a picture
    /// between two moments. In a cell because the walk that resolves them is an immutable reader.
    hits: Cell<u64>,
    /// The texels a frame handed to the compositor, by key.
    served: RefCell<FxHashSet<ImageKey>>,
    /// Uploaded attachments a frame found the compositor holds no image of.
    ///
    /// In a cell because the walk that finds them is an immutable reader. Drained by
    /// [`ContentCache::take_missing_images`], whose caller owns the sources and decodes them
    /// again.
    missing: RefCell<Vec<ReplacedId>>,
}

impl ContentCache {
    /// How many decoded bytes are referenced by replaced-node attachments.
    ///
    /// Shared attachments are counted once per node, so this is an attachment diagnostic rather
    /// than the window's allocation total; the runtime image loader owns and reports that
    /// deduplicated total. Externally owned textures count for nothing.
    pub fn image_bytes(&self) -> u64 {
        self.images.attached.values().map(Content::held_bytes).sum()
    }

    /// How many times a frame has resolved a replaced node to the content held for it.
    pub fn image_hits(&self) -> u64 {
        self.images.hits.get()
    }

    /// Detaches every image, and reports how many that threw away.
    ///
    /// This cache cannot put them back by itself. A runtime loader may retain their source and
    /// re-decode them; directly attached texels and external textures need their owner to attach
    /// them again.
    pub fn forget_images(&mut self) -> usize {
        let held = self.images.attached.len();
        self.images.attached.clear();
        held
    }

    /// Attaches decoded texels to a replaced node.
    ///
    /// The texels are premultiplied, gamma-encoded sRGB, four bytes per pixel, tightly packed with
    /// the top row first. They are held rather than only handed to the compositor, so that a
    /// compositor that dropped its image costs a write rather than a re-decode.
    ///
    /// # Errors
    ///
    /// [`ImageError::WrongByteCount`] when the buffer does not match the extent.
    pub fn set_image(
        &mut self,
        id: ReplacedId,
        size: Size<u32, Device>,
        texels: Vec<u8>,
    ) -> Result<(), ImageError> {
        let content = Content::decoded(id, size, texels)?;
        self.attach(id, content);
        Ok(())
    }

    /// Attaches already-shared texels to a replaced node, under a caller-chosen handle.
    ///
    /// Every node attached under one `handle` draws **one compositor image**, and the texels
    /// arrive behind an [`Arc`](std::sync::Arc), so a loader that decoded a file once serves any
    /// number of elements showing it without copying a byte. The texels obey the same contract as
    /// [`set_image`](ContentCache::set_image).
    ///
    /// # Errors
    ///
    /// [`ImageError::WrongByteCount`] when the buffer does not match the extent.
    pub fn set_image_shared(
        &mut self,
        id: ReplacedId,
        handle: u64,
        size: Size<u32, Device>,
        texels: std::sync::Arc<Vec<u8>>,
    ) -> Result<(), ImageError> {
        let content = Content::shared(handle, size, size, texels)?;
        self.attach(id, content);
        Ok(())
    }

    /// As [`set_image_shared`](ContentCache::set_image_shared), with the image's own extent.
    ///
    /// `natural` is the source's extent, which the texels may be a decode-for-display downscale
    /// of, and is what `object-fit: none` draws at.
    ///
    /// # Errors
    ///
    /// [`ImageError::WrongByteCount`] when the buffer does not match the extent.
    pub fn set_image_shared_natural(
        &mut self,
        id: ReplacedId,
        handle: u64,
        size: Size<u32, Device>,
        natural: Size<u32, Device>,
        texels: std::sync::Arc<Vec<u8>>,
    ) -> Result<(), ImageError> {
        let content = Content::shared(handle, size, natural, texels)?;
        self.attach(id, content);
        Ok(())
    }

    /// Attaches a replaced node to a shared image the compositor holds, carrying no texels.
    ///
    /// This is how a node joins a picture whose host copy has already been given back. If the
    /// compositor no longer holds the image, the node is reported through
    /// [`ContentCache::take_missing_images`] like any other uploaded attachment.
    pub fn set_image_uploaded(&mut self, id: ReplacedId, handle: u64, natural: Size<u32, Device>) {
        self.attach(id, Content::uploaded_shared(handle, natural));
    }

    /// Whether the compositor was handed the texels a loader's shared `handle` names.
    pub fn image_tile_resident(&self, handle: u64) -> bool {
        self.images.served.borrow().contains(&shared_key(handle))
    }

    /// Forgets that the compositor was handed the texels of a loader's shared `handle`.
    ///
    /// For the loader dropping a source entry: the handle leaves with the entry, so nothing can
    /// ever ask for its image again. The compositor removes the image once no chunk draws it.
    pub fn remove_shared_tile(&mut self, handle: u64) -> bool {
        self.images.served.get_mut().remove(&shared_key(handle))
    }

    /// Gives back the host texels of every shared attachment the compositor was handed.
    ///
    /// Call after the frame's transaction is committed. Attachments holding `keep_below` bytes or
    /// fewer keep their texels: a small picture's re-decode costs more in latency than its bytes
    /// cost in memory. Per-node attachments keep theirs whatever their size, since they were
    /// attached directly and nothing can decode them again.
    pub fn settle_uploaded(&mut self, keep_below: u64) {
        let ImageContent {
            attached, served, ..
        } = &mut self.images;
        let served = served.get_mut();
        for content in attached.values_mut() {
            if !content.is_shared() {
                continue;
            }
            let bytes = content.held_bytes();
            if bytes == 0 || bytes <= keep_below {
                continue;
            }
            let Content::Decoded { key, natural, .. } = content else {
                continue;
            };
            if !served.contains(key) {
                continue;
            }
            *content = Content::Uploaded {
                key: *key,
                natural: *natural,
            };
        }
    }

    /// The uploaded attachments recent frames found the compositor holds no image of, each at
    /// most once.
    pub fn take_missing_images(&mut self) -> Vec<ReplacedId> {
        let missing = std::mem::take(self.images.missing.get_mut());
        let mut seen = rustc_hash::FxHashSet::default();
        missing.into_iter().filter(|id| seen.insert(*id)).collect()
    }

    /// Attaches an external texture slot of the window's compositor client to a replaced node.
    pub fn set_external(&mut self, id: ReplacedId, texture: iceblink::TextureId) {
        self.attach(id, Content::External(texture));
    }

    /// Detaches whatever was attached to a replaced node.
    pub fn remove_image(&mut self, id: ReplacedId) {
        self.images.attached.remove(&id);
        self.note_image_change(id);
    }

    /// Attaches `content` to `id`.
    fn attach(&mut self, id: ReplacedId, content: Content) {
        self.images.attached.insert(id, content);
        self.note_image_change(id);
    }

    /// Moves a node's attachment revision, so a paint record kept across frames misses.
    fn note_image_change(&mut self, id: ReplacedId) {
        *self.images.revisions.entry(id).or_insert(0) += 1;
    }
}

impl ReplacedSource for FrameContent<'_> {
    fn revision(&self, id: ReplacedId) -> u64 {
        self.images.revisions.get(&id).copied().unwrap_or(0)
    }

    fn source(&self, id: ReplacedId) -> Option<Source> {
        let images = self.images;
        let content = images.attached.get(&id)?;
        images.hits.set(images.hits.get() + 1);
        // A shared handle names the same texels whatever node shows them.
        let revision = if content.is_shared() {
            0
        } else {
            self.revision(id)
        };
        Some(match content {
            Content::External(texture) => Source::External(*texture),
            Content::Decoded {
                key,
                size,
                natural,
                texels,
            } => {
                images.served.borrow_mut().insert(*key);
                Source::Decoded {
                    key: *key,
                    size: *size,
                    natural: *natural,
                    texels: Some(std::sync::Arc::clone(texels)),
                    revision,
                }
            }
            Content::Uploaded { key, natural } => Source::Decoded {
                key: *key,
                size: *natural,
                natural: *natural,
                texels: None,
                revision,
            },
        })
    }

    fn missing(&self, id: ReplacedId) {
        let images = self.images;
        if let Some(Content::Decoded { key, .. } | Content::Uploaded { key, .. }) =
            images.attached.get(&id)
        {
            images.served.borrow_mut().remove(key);
        }
        images.missing.borrow_mut().push(id);
    }
}
