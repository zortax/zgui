//! One decoded video frame as a producer hands it over: planes on the device, and what they mean.

use std::any::Any;
use std::sync::Arc;
use std::time::Instant;

use crate::wgpu;

use super::ColorSpace;
use super::sample::{ChromaSiting, SampleDepth};

/// The textures that hold one frame's samples.
///
/// Each plane is a 2D, single-sampled texture of a filterable float format with
/// `TEXTURE_BINDING` usage, on the device [`SurfaceEvent::Attached`](crate::SurfaceEvent)
/// delivered. `R8Unorm` and `Rg8Unorm` carry 8-bit video; `R16Unorm` and `Rg16Unorm` carry
/// deeper video, described by [`VideoFrame::with_depth`]. Chroma planes may be subsampled in
/// either direction; the subsampling is read from the plane sizes.
#[derive(Clone, Debug)]
pub enum Planes {
    /// Luma, and one plane that holds Cb in its first channel and Cr in its second. This is the
    /// NV12 and P010 layout most hardware decoders produce.
    Biplanar {
        /// The luma plane.
        luma: Arc<wgpu::Texture>,
        /// The interleaved chroma plane.
        chroma: Arc<wgpu::Texture>,
    },
    /// Luma, Cb and Cr in three planes. This is the I420, I422 and I444 layout most software
    /// decoders produce.
    Triplanar {
        /// The luma plane.
        luma: Arc<wgpu::Texture>,
        /// The Cb plane.
        cb: Arc<wgpu::Texture>,
        /// The Cr plane.
        cr: Arc<wgpu::Texture>,
    },
}

/// One plane of samples in memory.
#[derive(Clone, Copy, Debug)]
pub struct PlaneData<'a> {
    /// The rows, `stride` bytes apart.
    pub bytes: &'a [u8],
    /// The distance between the starts of two rows, in bytes.
    pub stride: u32,
    /// The plane's width in samples.
    pub width: u32,
    /// The plane's height in samples.
    pub height: u32,
}

/// The size of one sample in memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SampleSize {
    /// One byte per sample: 8-bit video.
    U8,
    /// Two native-endian bytes per sample: 10-, 12- and 16-bit video.
    U16,
}

/// A plane format the device cannot sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedFormat(pub wgpu::TextureFormat);

impl std::fmt::Display for UnsupportedFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the device cannot sample {:?} planes", self.0)
    }
}

impl std::error::Error for UnsupportedFormat {}

impl Planes {
    /// The luma plane, which sets the frame's sample grid.
    pub fn luma(&self) -> &Arc<wgpu::Texture> {
        match self {
            Self::Biplanar { luma, .. } | Self::Triplanar { luma, .. } => luma,
        }
    }

    /// The plane that holds Cb.
    pub(crate) fn chroma(&self) -> &Arc<wgpu::Texture> {
        match self {
            Self::Biplanar { chroma, .. } => chroma,
            Self::Triplanar { cb, .. } => cb,
        }
    }

    /// Every plane, luma first.
    pub(crate) fn all(&self) -> impl Iterator<Item = &Arc<wgpu::Texture>> {
        let (first, second, third) = match self {
            Self::Biplanar { luma, chroma } => (luma, chroma, None),
            Self::Triplanar { luma, cb, cr } => (luma, cb, Some(cr)),
        };
        [first, second].into_iter().chain(third)
    }

    /// Uploads luma, Cb and Cr planes from memory. Creates new textures on every call.
    ///
    /// # Errors
    ///
    /// [`SampleSize::U16`] planes need the device's `TEXTURE_FORMAT_16BIT_NORM` feature.
    pub fn upload_triplanar(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: SampleSize,
        [luma, cb, cr]: [PlaneData<'_>; 3],
    ) -> Result<Self, UnsupportedFormat> {
        let format = match size {
            SampleSize::U8 => wgpu::TextureFormat::R8Unorm,
            SampleSize::U16 => wgpu::TextureFormat::R16Unorm,
        };
        supported(device, format)?;
        Ok(Self::Triplanar {
            luma: upload(device, queue, "zgui.video.luma", format, luma),
            cb: upload(device, queue, "zgui.video.cb", format, cb),
            cr: upload(device, queue, "zgui.video.cr", format, cr),
        })
    }

    /// Uploads a luma plane and an interleaved Cb/Cr plane from memory. Creates new textures on
    /// every call.
    ///
    /// # Errors
    ///
    /// [`SampleSize::U16`] planes need the device's `TEXTURE_FORMAT_16BIT_NORM` feature.
    pub fn upload_biplanar(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: SampleSize,
        luma: PlaneData<'_>,
        chroma: PlaneData<'_>,
    ) -> Result<Self, UnsupportedFormat> {
        let (single, double) = match size {
            SampleSize::U8 => (wgpu::TextureFormat::R8Unorm, wgpu::TextureFormat::Rg8Unorm),
            SampleSize::U16 => (
                wgpu::TextureFormat::R16Unorm,
                wgpu::TextureFormat::Rg16Unorm,
            ),
        };
        supported(device, single)?;
        Ok(Self::Biplanar {
            luma: upload(device, queue, "zgui.video.luma", single, luma),
            chroma: upload(device, queue, "zgui.video.chroma", double, chroma),
        })
    }
}

/// Whether `device` can create a sampled plane of `format`.
fn supported(device: &wgpu::Device, format: wgpu::TextureFormat) -> Result<(), UnsupportedFormat> {
    let needed = format.required_features();
    if device.features().contains(needed) {
        Ok(())
    } else {
        Err(UnsupportedFormat(format))
    }
}

/// Creates one sampled plane and writes `data` into it.
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    format: wgpu::TextureFormat,
    data: PlaneData<'_>,
) -> Arc<wgpu::Texture> {
    let size = wgpu::Extent3d {
        width: data.width.max(1),
        height: data.height.max(1),
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        data.bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(data.stride),
            rows_per_image: None,
        },
        size,
    );
    Arc::new(texture)
}

/// One frame of video, ready for [`SurfaceHandle::present_video`](crate::SurfaceHandle).
///
/// zgui converts the planes to colour on its device during the frame that shows them.
pub struct VideoFrame {
    /// The samples.
    pub(crate) planes: Planes,
    /// What the samples mean.
    pub(crate) color: ColorSpace,
    /// How many bits each code has, and where they sit in a 16-bit sample.
    pub(crate) depth: Option<SampleDepth>,
    /// Where chroma samples sit relative to luma samples.
    pub(crate) siting: ChromaSiting,
    /// The brightest the content gets, in nits, for tone mapping HDR.
    pub(crate) peak: Option<f32>,
    /// The visible part of the luma plane, from its top-left corner.
    pub(crate) visible: Option<(u32, u32)>,
    /// When the frame is meant to be on the screen.
    pub(crate) at: Option<Instant>,
    /// What the producer keeps alive until the device finished reading the planes.
    pub(crate) guard: Option<Box<dyn Any + Send>>,
}

impl VideoFrame {
    /// A frame of `planes` whose samples mean `color`.
    pub fn new(planes: Planes, color: ColorSpace) -> Self {
        Self {
            planes,
            color,
            depth: None,
            siting: ChromaSiting::default(),
            peak: None,
            visible: None,
            at: None,
            guard: None,
        }
    }

    /// States how many bits each code has, and where they sit in a 16-bit sample.
    ///
    /// Without it, an 8-bit plane holds 8-bit codes and a 16-bit plane holds 16-bit codes.
    #[must_use]
    pub fn with_depth(mut self, depth: SampleDepth) -> Self {
        self.depth = Some(depth);
        self
    }

    /// States where chroma samples sit relative to luma samples. The default is
    /// [`ChromaSiting::Left`], the position H.264, HEVC and most decoders use.
    #[must_use]
    pub fn with_chroma_siting(mut self, siting: ChromaSiting) -> Self {
        self.siting = siting;
        self
    }

    /// States the brightest the content gets, in nits.
    ///
    /// HDR content above reference white is compressed into the range the compositor can show,
    /// and this sets how much. Use the stream's maximum content light level, or its mastering
    /// display's peak. Without it, HDR content is assumed to peak at 1000 nits.
    #[must_use]
    pub fn with_peak_luminance(mut self, nits: f32) -> Self {
        self.peak = Some(nits);
        self
    }

    /// Shows only the top-left `width` × `height` luma samples.
    ///
    /// Decoders pad planes to their block size. This states the picture's real size, which is
    /// also the size of the texture the surface shows. Values larger than the luma plane are
    /// clamped to it.
    #[must_use]
    pub fn with_visible_size(mut self, width: u32, height: u32) -> Self {
        self.visible = Some((width, height));
        self
    }

    /// Stamps the frame with the moment it is meant to be on the screen.
    ///
    /// A stamped frame queues: it shows on the refresh nearest its stamp, frames stamped earlier
    /// leave unshown once it does, and the window wakes for it without running a frame on every
    /// refresh in between. A late frame shows on the next refresh. Up to eight frames queue per
    /// surface; the earliest leaves when a ninth arrives. Stamp from the same clock as
    /// [`Instant::now`], with the playback clock's offset applied.
    ///
    /// An unstamped frame shows on the next refresh and empties the queue.
    #[must_use]
    pub fn with_presentation_time(mut self, at: Instant) -> Self {
        self.at = Some(at);
        self
    }

    /// Keeps `guard` alive until the device finished reading this frame's planes, then drops it.
    ///
    /// A plane imported from a decoder's own memory is valid only while the decoder's buffer is
    /// held. Hand that buffer over here, and zgui releases it at the earliest safe moment. A frame
    /// that is replaced before it is shown releases its guard at once.
    #[must_use]
    pub fn with_guard(mut self, guard: impl Send + 'static) -> Self {
        self.guard = Some(Box::new(guard));
        self
    }

    /// The planes.
    pub fn planes(&self) -> &Planes {
        &self.planes
    }

    /// What the samples mean.
    pub fn color(&self) -> ColorSpace {
        self.color
    }

    /// The size of the picture this frame shows, in luma samples.
    pub fn size(&self) -> (u32, u32) {
        let luma = self.planes.luma();
        let (width, height) = (luma.width(), luma.height());
        match self.visible {
            Some((w, h)) => (w.clamp(1, width), h.clamp(1, height)),
            None => (width, height),
        }
    }

    /// The code depth, stated or read from the luma plane's format.
    pub(crate) fn depth(&self) -> SampleDepth {
        self.depth
            .unwrap_or_else(|| SampleDepth::of_format(self.planes.luma().format()))
    }
}

impl std::fmt::Debug for VideoFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoFrame")
            .field("planes", &self.planes)
            .field("color", &self.color)
            .field("depth", &self.depth())
            .field("siting", &self.siting)
            .field("size", &self.size())
            .field("at", &self.at)
            .field("guarded", &self.guard.is_some())
            .finish()
    }
}
