//! Where a composed frame is copied to.

use zgui_geom::{Device, Size};

use crate::gpu::device::Gpu;
use crate::gpu::formats::{self, Formats};
use crate::gpu::surface::ConfiguredSurface;
use crate::target::acquire::Acquisition;

/// A texture a frame can be copied into, and the answer that produced it.
pub struct Presented {
    /// Which answer the request got.
    pub acquisition: Acquisition,
    /// The acquired surface texture, when one came back and has to be presented.
    pub surface_texture: Option<wgpu::SurfaceTexture>,
    /// The view to copy into, when there is one.
    pub view: Option<wgpu::TextureView>,
}

/// What a frame is presented to.
///
/// A window's surface, or a texture standing in for one. The second is not a stub: it is
/// configured from the same format rules, copied into by the same pipeline through the same view,
/// and read back byte for byte — which is what lets the encoding decisions above be *measured*
/// rather than argued, on a machine with no window.
///
/// The third is a set of textures the caller owns and points at one of before each frame. It is
/// how a display controller's own scanout buffers are drawn into: the copy lands in the buffer the
/// hardware will read, so no frame passes through the processor on its way to a screen.
#[derive(Debug)]
pub enum Presentation {
    /// A real surface, acquired from and presented to every frame.
    Surface(Box<ConfiguredSurface>),
    /// A texture, standing in for a surface that cannot be created.
    Offscreen(Offscreen),
    /// Textures a caller owns, one of which it chooses before each frame.
    Supplied(Supplied),
}

impl Presentation {
    /// The formats everything is drawn in.
    pub fn formats(&self) -> Formats {
        match self {
            Self::Surface(surface) => surface.formats(),
            Self::Offscreen(offscreen) => offscreen.formats,
            Self::Supplied(supplied) => supplied.formats,
        }
    }

    /// The extent being presented at.
    pub fn size(&self) -> Size<i32, Device> {
        match self {
            Self::Surface(surface) => surface.size(),
            Self::Offscreen(offscreen) => offscreen.size,
            Self::Supplied(supplied) => supplied.size,
        }
    }

    /// The presentation mode a real surface holds; a target with no swap chain has none.
    pub fn present_mode(&self) -> Option<wgpu::PresentMode> {
        match self {
            Self::Surface(surface) => Some(surface.present_mode()),
            Self::Offscreen(_) => None,
            // A caller that supplies the textures also decides when one is shown, so there is no
            // swap chain here to hold a mode.
            Self::Supplied(_) => None,
        }
    }

    /// Whether a frame may be recorded against this at all.
    pub fn is_configured(&self) -> bool {
        match self {
            Self::Surface(surface) => surface.is_configured(),
            // A texture is ready as soon as it exists, whoever created it: there is no swap chain
            // to negotiate and nothing that waits for a device.
            Self::Offscreen(_) | Self::Supplied(_) => true,
        }
    }

    /// Resizes, recording a surface's new extent or reallocating a texture.
    ///
    /// A surface's swap chain is not rebuilt here — [`Presentation::apply_pending`] does that, at
    /// the one point in a frame where the wait it costs overlaps work already done.
    ///
    /// A supplied set is refused instead, because the renderer did not create those textures and
    /// cannot create another. A display's mode holds still while a program runs, and the caller
    /// that owns the buffers supplies a new set when it changes.
    pub fn resize(&mut self, gpu: &Gpu, size: Size<i32, Device>) {
        match self {
            Self::Surface(surface) => surface.resize(size),
            Self::Offscreen(offscreen) => *offscreen = offscreen.resized(gpu, size),
            Self::Supplied(supplied) => tracing::warn!(
                width = size.width,
                height = size.height,
                supplied_width = supplied.size.width,
                supplied_height = supplied.size.height,
                "supplied textures cannot be resized; the caller supplies another set"
            ),
        }
    }

    /// Rebuilds a surface's swap chain if one is owed.
    ///
    /// A texture owes nothing, supplied or standing in for a surface: nothing about either waits
    /// for a device.
    pub fn apply_pending(&mut self, gpu: &Gpu) {
        if let Self::Surface(surface) = self {
            surface.apply(gpu);
        }
    }

    /// Asks for something to copy this frame into.
    pub fn acquire(&self) -> Presented {
        match self {
            Self::Surface(surface) => {
                let acquired = surface.surface().get_current_texture();
                let acquisition = Acquisition::classify(&acquired);
                let surface_texture = match acquired {
                    wgpu::CurrentSurfaceTexture::Success(texture)
                    | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => Some(texture),
                    _ => None,
                };
                let view = surface_texture
                    .as_ref()
                    .map(|texture| surface.present_view(&texture.texture));
                Presented {
                    acquisition,
                    surface_texture,
                    view,
                }
            }
            Self::Offscreen(offscreen) => Presented {
                acquisition: Acquisition::Success,
                surface_texture: None,
                view: Some(offscreen.view()),
            },
            // The caller already chose which texture this is, so there is nothing to ask for and
            // nothing to hand back afterwards: whoever owns the textures presents them.
            Self::Supplied(supplied) => Presented {
                acquisition: Acquisition::Success,
                surface_texture: None,
                view: Some(supplied.view()),
            },
        }
    }

    /// Notes that the surface asked to be reconfigured at the next opportunity.
    pub fn request_reconfigure(&mut self) {
        if let Self::Surface(surface) = self {
            surface.request_reconfigure();
        }
    }

    /// Whether a reconfiguration is owed before the next frame.
    pub fn reconfigure_pending(&self) -> bool {
        match self {
            Self::Surface(surface) => surface.reconfigure_pending(),
            // Only a swap chain is ever owed a reconfiguration, and neither of these is one.
            Self::Offscreen(_) | Self::Supplied(_) => false,
        }
    }

    /// Marks the configuration as no longer describing the window.
    pub fn invalidate(&mut self) {
        if let Self::Surface(surface) = self {
            surface.invalidate();
        }
    }
}

/// A texture standing in for a surface.
///
/// It exists so that a frame can be composed, copied and read back with no window: the pixel
/// suites, the startup pattern and the encoding measurements all run through this. Its format is
/// chosen by the same rules a real surface's is, so a test can present into an encoded target and
/// watch the fallback tier cancel the encode.
#[derive(Debug)]
pub struct Offscreen {
    /// The texture.
    texture: wgpu::Texture,
    /// Its extent.
    size: Size<i32, Device>,
    /// The formats derived for it.
    formats: Formats,
}

impl Offscreen {
    /// The usage a stand-in surface needs: copied into, and copied out of by a test.
    const USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT
        .union(wgpu::TextureUsages::COPY_SRC)
        .union(wgpu::TextureUsages::TEXTURE_BINDING);

    /// A stand-in surface of `size` presenting in `format`.
    ///
    /// `mutable_texture_formats` is what a device would have reported; passing it explicitly is
    /// what lets both fallbacks for an encoded surface be exercised on one machine.
    pub fn new(
        gpu: &Gpu,
        size: Size<i32, Device>,
        format: wgpu::TextureFormat,
        mutable_texture_formats: bool,
    ) -> Self {
        let formats = formats::choose(
            &[format],
            &[wgpu::CompositeAlphaMode::Opaque],
            true,
            mutable_texture_formats,
        );
        debug_assert!(
            formats.is_sound(),
            "an encoded stand-in surface with nothing to cancel the encode: {formats:?}"
        );
        let view_formats = formats.view_formats();
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: Some("zgui.offscreen"),
            size: wgpu::Extent3d {
                width: size.width.max(1) as u32,
                height: size.height.max(1) as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: formats.surface,
            usage: Self::USAGE,
            view_formats: &view_formats,
        });
        Self {
            texture,
            size: size.non_negative(),
            formats,
        }
    }

    /// The same stand-in surface at a new extent.
    fn resized(&self, gpu: &Gpu, size: Size<i32, Device>) -> Self {
        Self::new(
            gpu,
            size,
            self.formats.surface,
            self.formats.view_format_twin.is_some(),
        )
    }

    /// The texture, for a copy out of it.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// The formats it was derived with.
    pub fn formats(&self) -> Formats {
        self.formats
    }

    /// The view a frame is copied into, through the unencoded twin where there is one.
    fn view(&self) -> wgpu::TextureView {
        match self.formats.view_format_twin {
            Some(format) => self.texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(format),
                ..Default::default()
            }),
            None => self
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        }
    }
}

/// Textures a caller supplies and rotates between.
///
/// What a display controller scans out of. The buffers belong to whatever drives the display, and
/// there are several of them so that the hardware reads one while the next is drawn. The caller
/// points at one before each frame, and the renderer copies its composed target straight into the
/// buffer that reaches the screen, so no frame is read back and none is copied through memory.
#[derive(Debug)]
pub struct Supplied {
    /// The textures, in the order the caller gave them.
    textures: Vec<wgpu::Texture>,
    /// Which one the next frame is copied into.
    selected: usize,
    /// The extent every one of them has.
    size: Size<i32, Device>,
    /// The formats derived from them.
    formats: Formats,
}

impl Supplied {
    /// A presentation over `textures`, of which the first is written next.
    ///
    /// `size` is the extent the presentation reports, and it has to be the extent the textures
    /// have. Every texture needs [`wgpu::TextureUsages::RENDER_ATTACHMENT`], because the copy that
    /// ends a frame renders into a view of it, and `COPY_SRC` as well where a caller reads one
    /// back.
    ///
    /// # Refusals
    ///
    /// Answers `None` for an empty set, for textures that disagree about their format or their
    /// extent, and for a `size` that disagrees with them. One [`Formats`] and one extent are
    /// derived for the whole set, so a set that disagrees would present the frames that land on
    /// one texture correctly and corrupt the frames that land on another.
    pub fn new(textures: Vec<wgpu::Texture>, size: Size<i32, Device>) -> Option<Self> {
        let first = textures.first()?;
        let format = first.format();
        let extent = first.size();
        let size = size.non_negative();
        if extent.width != size.width as u32 || extent.height != size.height as u32 {
            tracing::error!(
                width = extent.width,
                height = extent.height,
                stated_width = size.width,
                stated_height = size.height,
                "the supplied textures are not the extent they were supplied at"
            );
            return None;
        }
        if let Some(odd) = textures
            .iter()
            .find(|texture| texture.format() != format || texture.size() != extent)
        {
            tracing::error!(
                format = ?format,
                odd_format = ?odd.format(),
                extent = ?extent,
                odd_extent = ?odd.size(),
                "the supplied textures do not agree, so they cannot be presented to as one set"
            );
            return None;
        }
        // Derived from the textures themselves. The texture already answers its format, and a
        // second statement of it beside them is a way for the two to disagree.
        //
        // No mutable-format view is claimed, because a texture that exists cannot be given another
        // view format afterwards. An encoded texture therefore has its encode cancelled in the copy
        // that ends a frame, which asks nothing of the texture at all.
        let formats = formats::choose(&[format], &[wgpu::CompositeAlphaMode::Opaque], true, false);
        debug_assert!(
            formats.is_sound(),
            "an encoded supplied texture with nothing to cancel the encode: {formats:?}"
        );
        Some(Self {
            textures,
            selected: 0,
            size,
            formats,
        })
    }

    /// Points at the texture the next frame is copied into.
    ///
    /// Answers whether the slot exists. A slot outside the set leaves the selection where it was
    /// and never wraps, because a wrapped slot is a buffer the caller did not choose — and the
    /// caller is the only party that knows which buffers the display controller has finished with.
    /// Guessing one here would put a frame on a buffer for reasons of arithmetic.
    pub fn select(&mut self, slot: usize) -> bool {
        if slot >= self.textures.len() {
            tracing::warn!(
                slot,
                supplied = self.textures.len(),
                "a slot outside the supplied textures was asked for, so the selection stands"
            );
            return false;
        }
        self.selected = slot;
        true
    }

    /// Returns which texture the next frame is copied into.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Returns how many textures a caller supplied.
    ///
    /// Always one or more: [`Supplied::new`] refuses an empty set.
    #[allow(
        clippy::len_without_is_empty,
        reason = "an empty set is refused at construction, so the question has one answer"
    )]
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Returns the texture the next frame is copied into, for a copy out of it.
    pub fn texture(&self) -> &wgpu::Texture {
        &self.textures[self.selected]
    }

    /// Returns the formats derived from the textures.
    pub fn formats(&self) -> Formats {
        self.formats
    }

    /// Returns the view a frame is copied into.
    ///
    /// The texture's own format, always: a supplied set claims no unencoded twin, because the view
    /// format of a texture is fixed when the texture is created.
    fn view(&self) -> wgpu::TextureView {
        self.texture()
            .create_view(&wgpu::TextureViewDescriptor::default())
    }
}
