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
            // A texture the renderer allocated is ready as soon as it exists: there is no swap
            // chain to negotiate and nothing that waits for a device.
            Self::Offscreen(_) => true,
            Self::Supplied(supplied) => supplied.is_configured(),
        }
    }

    /// Resizes, recording a surface's new extent or reallocating a texture.
    ///
    /// A surface's swap chain is not rebuilt here — [`Presentation::apply_pending`] does that, at
    /// the one point in a frame where the wait it costs overlaps work already done.
    ///
    /// A supplied set is only told, because the renderer did not create those textures and cannot
    /// create another. A display's mode holds still while a program runs, and the caller that owns
    /// the buffers supplies a new set when it changes; until it does, the set and the target
    /// disagree and [`Presentation::is_configured`] answers `false`.
    pub fn resize(&mut self, gpu: &Gpu, size: Size<i32, Device>) {
        match self {
            Self::Surface(surface) => surface.resize(size),
            Self::Offscreen(offscreen) => *offscreen = offscreen.resized(gpu, size),
            Self::Supplied(supplied) => supplied.retarget(size),
        }
    }

    /// Rebuilds a surface's swap chain if one is owed.
    pub fn apply_pending(&mut self, gpu: &Gpu) {
        match self {
            Self::Surface(surface) => surface.apply(gpu),
            // A texture owes nothing, supplied or standing in for a surface: nothing about either
            // waits for a device. A supplied set that disagrees with the target is not waiting for
            // this either — only the caller can end that, by supplying a set at the new extent.
            Self::Offscreen(_) | Self::Supplied(_) => {}
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
        match self {
            Self::Surface(surface) => surface.request_reconfigure(),
            // Only a swap chain has a configuration to rebuild.
            Self::Offscreen(_) | Self::Supplied(_) => {}
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
        match self {
            Self::Surface(surface) => surface.invalidate(),
            // Neither describes a window, so neither can stop describing one.
            Self::Offscreen(_) | Self::Supplied(_) => {}
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
    /// Whether the renderer was configured for an extent these textures do not have.
    ///
    /// The renderer cannot reallocate a supplied set, so this is a state a frame has to stop in
    /// rather than one it can draw through. The copy that ends a frame covers the whole buffer
    /// while reading a composed target of the other extent, which would put a correct corner, a
    /// stretch and a black remainder on a screen, every frame, with nothing to say so.
    diverged: bool,
}

impl Supplied {
    /// Creates a presentation over `textures`, of which the first is written next.
    ///
    /// The extent is the textures' own. Answers `None` when they cannot be presented to as one
    /// set; [`Supplied::unusable`] says why, and this writes that reason to the log.
    pub fn new(textures: Vec<wgpu::Texture>) -> Option<Self> {
        if let Some(reason) = Self::unusable(&textures) {
            tracing::error!(%reason, "the supplied textures were refused");
            return None;
        }
        let first = textures.first()?;
        let size = Size::new(first.width() as i32, first.height() as i32);
        // Derived from the textures themselves. The texture already answers its format, and a
        // second statement of it beside them is a way for the two to disagree.
        //
        // No mutable-format view is claimed, because a texture that exists cannot be given another
        // view format afterwards. An encoded texture therefore has its encode cancelled in the copy
        // that ends a frame, which asks nothing of the texture at all.
        let formats = formats::choose(
            &[first.format()],
            &[wgpu::CompositeAlphaMode::Opaque],
            true,
            false,
        );
        debug_assert!(
            formats.is_sound(),
            "an encoded supplied texture with nothing to cancel the encode: {formats:?}"
        );
        Some(Self {
            textures,
            selected: 0,
            size,
            formats,
            diverged: false,
        })
    }

    /// Returns why `textures` cannot be presented to as one set, or `None` when they can.
    ///
    /// Every question here is answered by the handle itself, and every answer is otherwise fatal:
    /// wgpu's default uncaptured-error handler panics, so a texture that cannot be a colour
    /// attachment takes the program down inside the first frame's render pass. So the questions
    /// are asked while the caller can still act on the answer.
    ///
    /// The set also has to agree with itself, because one [`Formats`] and one extent are derived
    /// for all of it. A set that disagreed would present the frames landing on one texture
    /// correctly and corrupt the frames landing on another.
    ///
    /// ```
    /// use zgui_render_wgpu::target::swapchain::Supplied;
    ///
    /// // The one refusal that needs no device to reach: a set with nothing in it.
    /// assert!(Supplied::unusable(&[]).is_some());
    /// ```
    pub fn unusable(textures: &[wgpu::Texture]) -> Option<String> {
        let Some(first) = textures.first() else {
            return Some("the set is empty, so it states no format and no extent".to_owned());
        };
        for (slot, texture) in textures.iter().enumerate() {
            if !texture
                .usage()
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            {
                return Some(format!(
                    "texture {slot} is {:?} and a frame is copied into it, which needs \
                     RENDER_ATTACHMENT",
                    texture.usage()
                ));
            }
            if texture.dimension() != wgpu::TextureDimension::D2 {
                return Some(format!(
                    "texture {slot} is {:?}; a frame is copied into a two-dimensional attachment",
                    texture.dimension()
                ));
            }
            if texture.mip_level_count() != 1 {
                return Some(format!(
                    "texture {slot} has {} mip levels; a frame is copied into one",
                    texture.mip_level_count()
                ));
            }
            if texture.sample_count() != 1 {
                return Some(format!(
                    "texture {slot} takes {} samples; the copy that ends a frame resolves nothing",
                    texture.sample_count()
                ));
            }
            if texture.depth_or_array_layers() != 1 {
                return Some(format!(
                    "texture {slot} has {} layers; a frame is copied into one",
                    texture.depth_or_array_layers()
                ));
            }
            if texture.format() != first.format() {
                return Some(format!(
                    "texture {slot} is {:?} where the first is {:?}; one set is presented in one \
                     format",
                    texture.format(),
                    first.format()
                ));
            }
            if texture.size() != first.size() {
                return Some(format!(
                    "texture {slot} is {}×{} where the first is {}×{}; one set is presented at one \
                     extent",
                    texture.width(),
                    texture.height(),
                    first.width(),
                    first.height()
                ));
            }
        }
        None
    }

    /// Points at the texture the next frame is copied into.
    ///
    /// Answers whether the slot exists. A slot outside the set leaves the selection where it was
    /// and never wraps, because a wrapped slot is a buffer the caller did not choose — and the
    /// caller is the only party that knows which buffers the display controller has finished with.
    /// Guessing one here would put a frame on a buffer for reasons of arithmetic.
    #[must_use = "a refused slot leaves the frame going where it was already going"]
    pub fn select(&mut self, slot: usize) -> bool {
        if slot >= self.textures.len() {
            tracing::warn!(
                slot,
                supplied = self.textures.len(),
                selected = self.selected,
                "a slot outside the supplied textures was asked for, so the selection stands"
            );
            return false;
        }
        self.selected = slot;
        true
    }

    /// Records the extent the renderer was configured for.
    ///
    /// A supplied set cannot be reallocated, so a target of another extent puts the two out of
    /// step rather than resizing anything. That is recorded rather than performed:
    /// [`Supplied::is_configured`] answers `false` until the extents agree again, which stops
    /// frames instead of stretching them across a buffer.
    fn retarget(&mut self, size: Size<i32, Device>) {
        let wanted = Size::new(size.width.max(1), size.height.max(1));
        let diverged = wanted != self.size;
        if diverged != self.diverged {
            if diverged {
                tracing::warn!(
                    width = wanted.width,
                    height = wanted.height,
                    supplied_width = self.size.width,
                    supplied_height = self.size.height,
                    "the renderer was configured for an extent the supplied textures do not have; \
                     frames stop until a set at that extent is supplied"
                );
            } else {
                tracing::info!("the target agrees with the supplied textures again");
            }
        }
        self.diverged = diverged;
    }

    /// Returns `true` while a frame may be copied into this set.
    ///
    /// `false` once the renderer has been configured for an extent these textures do not have. The
    /// caller ends that by supplying a set at the new extent, or by configuring the target back.
    pub fn is_configured(&self) -> bool {
        !self.diverged
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

    /// Returns the extent every texture in the set has.
    ///
    /// The textures' own, read off the first of them. A caller comparing this against what it
    /// intends to present is comparing against the buffers themselves.
    pub fn size(&self) -> Size<i32, Device> {
        self.size
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
