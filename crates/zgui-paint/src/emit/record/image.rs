//! Images the recorder names, created in the compositor from zgui's texels.

use compositor_types::{Image, ImageDesc, ImageWrite};

use crate::emit::record::{Recorder, Ref, refer};

/// The zgui name of an image's texels.
///
/// Equal keys name equal texels at equal revisions, so every chunk that draws one key draws one
/// compositor image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageKey(pub u64);

impl ImageKey {
    /// The key namespace of atlas textures.
    const TEXTURES: u64 = 0x7C00_0000_0000_0000;

    /// Returns the key of the image a mask clip over a tile of `texture` samples.
    pub fn of_texture(texture: zgui_atlas::TextureId) -> Self {
        Self(Self::TEXTURES | u64::from(texture.kind as u8) << 32 | u64::from(texture.index))
    }
}

/// The texels of an image, for the recorder to create or update the compositor's copy.
#[derive(Clone, Copy, Debug)]
pub struct ImagePixels<'a> {
    /// The image's extent and texel format.
    pub desc: ImageDesc,
    /// A number that moves whenever the texels under the key change.
    pub revision: u64,
    /// The texels, tightly packed with the top row first, or `None` for transparent texels
    /// written later.
    pub texels: Option<&'a [u8]>,
}

impl Recorder<'_> {
    /// Returns the slot an item drawing the image `key` holds, and the image's extent in texels.
    ///
    /// An image the compositor holds no copy of is created from `pixels`, and a copy of another
    /// revision is brought up to date.
    ///
    /// Returns `None` when the compositor holds no image of `key` and `pixels` is `None`. Without
    /// a compositor every image is slot zero, of the extent `pixels` gives.
    pub fn image_slot(
        &mut self,
        key: ImageKey,
        pixels: Option<ImagePixels<'_>>,
    ) -> Option<(u32, [u32; 2])> {
        let Some(resolver) = &mut self.resolver else {
            let extent = pixels.map_or([0; 2], |pixels| [pixels.desc.width, pixels.desc.height]);
            return Some((0, extent));
        };
        let (id, desc) = resolver.image(key, pixels)?;
        refer(self.items, Ref::Image(id));
        Some((id.slot(), [desc.width, desc.height]))
    }

    /// Appends an image item, whose `image` is a slot [`Recorder::image_slot`] returned.
    pub fn image(&mut self, image: Image) {
        self.items.push_image(image);
    }

    /// Writes `write` into the compositor's copy of the image `key`, which then holds the texels
    /// of `revision`, and reports whether there is one.
    pub fn write_image(&mut self, key: ImageKey, write: ImageWrite, revision: u64) -> bool {
        self.resolver
            .as_mut()
            .is_some_and(|resolver| resolver.write_image(key, write, revision))
    }
}
