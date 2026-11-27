//! zgui's images mirrored as compositor images.

use std::sync::Arc;

use iceblink::geom::{Point, Size};
use iceblink::{DeviceRect, ImageDesc, ImageId, ImageWrite, Transaction};
use rustc_hash::FxHashMap;

use crate::emit::record::{ImageKey, ImagePixels};
use crate::sync::mirror::holds::Holds;

/// The compositor image of each mirrored zgui image, by key.
#[derive(Debug, Default)]
pub(crate) struct Images {
    entries: FxHashMap<ImageKey, Entry>,
    keys: FxHashMap<ImageId, ImageKey>,
}

/// One mirrored image, and the texels it was last written with.
#[derive(Clone, Copy, Debug)]
struct Entry {
    id: ImageId,
    desc: ImageDesc,
    revision: u64,
}

impl Images {
    /// Returns the image of `key` for the chunk being recorded.
    ///
    /// The image depends on what is mirrored and on `pixels`:
    ///
    /// - If nothing is mirrored for `key`, an image is created from `pixels`.
    /// - If the mirrored image has another revision of the same extent and format, the new texels
    ///   are written into it.
    /// - If it has another extent or format, a new image takes its place. The old one lives while
    ///   a chunk names it.
    /// - Otherwise the mirrored image is returned as it is.
    ///
    /// Returns the image and its description, or `None` when nothing is mirrored for `key` and
    /// `pixels` is `None`.
    pub(crate) fn ensure(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        key: ImageKey,
        pixels: Option<ImagePixels<'_>>,
    ) -> Option<(ImageId, ImageDesc)> {
        let (id, desc) = match (self.entries.get_mut(&key), pixels) {
            (Some(entry), None) => (entry.id, entry.desc),
            (Some(entry), Some(pixels)) if pixels.revision == entry.revision => {
                (entry.id, entry.desc)
            }
            (Some(entry), Some(pixels)) if pixels.desc == entry.desc => {
                if let Some(texels) = pixels.texels {
                    tx.write_image(entry.id, whole(pixels.desc, texels));
                }
                entry.revision = pixels.revision;
                (entry.id, entry.desc)
            }
            (_, Some(pixels)) => (self.create(tx, holds, key, pixels), pixels.desc),
            (None, None) => return None,
        };
        holds.name(id);
        Some((id, desc))
    }

    /// Writes `write` into the image of `key`, which then holds the texels of `revision`, and
    /// reports whether one is mirrored.
    pub(crate) fn write(
        &mut self,
        tx: &mut Transaction,
        key: ImageKey,
        write: ImageWrite,
        revision: u64,
    ) -> bool {
        let Some(entry) = self.entries.get_mut(&key) else {
            return false;
        };
        tx.write_image(entry.id, write);
        entry.revision = revision;
        true
    }

    /// Reports whether an image is mirrored for `key`.
    pub(crate) fn holds(&self, key: ImageKey) -> bool {
        self.entries.contains_key(&key)
    }

    /// Forgets the image `id`, which was removed.
    pub(crate) fn removed(&mut self, id: ImageId) {
        let Some(key) = self.keys.remove(&id) else {
            return;
        };
        if self.entries.get(&key).is_some_and(|entry| entry.id == id) {
            self.entries.remove(&key);
        }
    }

    fn create(
        &mut self,
        tx: &mut Transaction,
        holds: &mut Holds,
        key: ImageKey,
        pixels: ImagePixels<'_>,
    ) -> ImageId {
        let id = tx.create_image(pixels.desc);
        if let Some(texels) = pixels.texels {
            tx.write_image(id, whole(pixels.desc, texels));
        }
        holds.created(id);
        self.keys.insert(id, key);
        self.entries.insert(
            key,
            Entry {
                id,
                desc: pixels.desc,
                revision: pixels.revision,
            },
        );
        id
    }
}

/// Returns a write of every texel of an image of `desc`, tightly packed in `texels`.
pub(crate) fn whole(desc: ImageDesc, texels: &[u8]) -> ImageWrite {
    ImageWrite {
        rect: DeviceRect::new(
            Point::new(0, 0),
            Size::new(desc.width as i32, desc.height as i32),
        ),
        bytes: Arc::from(texels),
        offset: 0,
        stride: desc.width * bytes_per_texel(desc),
    }
}

/// Returns the bytes one texel of an image of `desc` takes.
pub(crate) fn bytes_per_texel(desc: ImageDesc) -> u32 {
    if desc.format == iceblink::ImageFormat::A8 {
        1
    } else {
        4
    }
}
