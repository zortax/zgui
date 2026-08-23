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
#[derive(Debug)]
pub enum Presentation {
    /// A real surface, acquired from and presented to every frame.
    Surface(Box<ConfiguredSurface>),
    /// A texture, standing in for a surface that cannot be created.
    Offscreen(Offscreen),
}

impl Presentation {
    /// The formats everything is drawn in.
    pub fn formats(&self) -> Formats {
        match self {
            Self::Surface(surface) => surface.formats(),
            Self::Offscreen(offscreen) => offscreen.formats,
        }
    }

    /// The extent being presented at.
    pub fn size(&self) -> Size<i32, Device> {
        match self {
            Self::Surface(surface) => surface.size(),
            Self::Offscreen(offscreen) => offscreen.size,
        }
    }

    /// The presentation mode a real surface holds; nothing offscreen has one.
    pub fn present_mode(&self) -> Option<wgpu::PresentMode> {
        match self {
            Self::Surface(surface) => Some(surface.present_mode()),
            Self::Offscreen(_) => None,
        }
    }

    /// Whether a frame may be recorded against this at all.
    pub fn is_configured(&self) -> bool {
        match self {
            Self::Surface(surface) => surface.is_configured(),
            Self::Offscreen(_) => true,
        }
    }

    /// Resizes, recording a surface's new extent or reallocating a texture.
    ///
    /// A surface's swap chain is not rebuilt here — [`Presentation::apply_pending`] does that, at
    /// the one point in a frame where the wait it costs overlaps work already done.
    pub fn resize(&mut self, gpu: &Gpu, size: Size<i32, Device>) {
        match self {
            Self::Surface(surface) => surface.resize(size),
            Self::Offscreen(offscreen) => *offscreen = offscreen.resized(gpu, size),
        }
    }

    /// Rebuilds a surface's swap chain if one is owed.
    ///
    /// A texture standing in for a surface owes nothing: it was reallocated where it was resized,
    /// because nothing about it waits for a device.
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
            Self::Offscreen(_) => false,
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
