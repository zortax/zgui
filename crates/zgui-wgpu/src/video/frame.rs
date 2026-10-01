//! One decoded video frame as a producer hands it over: planes on the device, and what they mean.

use std::any::Any;
use std::sync::Arc;

use crate::wgpu;

use super::ColorSpace;

/// The textures that hold one frame's samples.
///
/// Each plane is a 2D, single-sampled texture of a filterable float format with
/// `TEXTURE_BINDING` usage, on the device [`SurfaceEvent::Attached`](crate::SurfaceEvent)
/// delivered. `R8Unorm` and `Rg8Unorm` carry 8-bit video; `R16Unorm` and `Rg16Unorm` carry
/// 10-bit and 12-bit video stored in the high bits. Chroma planes may be subsampled: they are read
/// at the same normalised position as the luma plane.
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
    /// Luma, Cb and Cr in three planes. This is the I420 and I444 layout most software decoders
    /// produce.
    Triplanar {
        /// The luma plane.
        luma: Arc<wgpu::Texture>,
        /// The Cb plane.
        cb: Arc<wgpu::Texture>,
        /// The Cr plane.
        cr: Arc<wgpu::Texture>,
    },
}

impl Planes {
    /// The luma plane, which sets the frame's sample grid.
    pub fn luma(&self) -> &Arc<wgpu::Texture> {
        match self {
            Self::Biplanar { luma, .. } | Self::Triplanar { luma, .. } => luma,
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

    /// Uploads 8-bit I420 planes from memory: full-size luma, then Cb and Cr at half size in each
    /// direction, rounded up.
    ///
    /// Each plane is a byte slice and the distance in bytes between its rows. Creates new
    /// textures on every call.
    pub fn upload_i420(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        luma: (&[u8], u32),
        cb: (&[u8], u32),
        cr: (&[u8], u32),
    ) -> Self {
        let (chroma_width, chroma_height) = (width.div_ceil(2), height.div_ceil(2));
        let plane = |label, (bytes, stride), w, h| {
            upload(
                device,
                queue,
                label,
                wgpu::TextureFormat::R8Unorm,
                (w, h),
                bytes,
                stride,
            )
        };
        Self::Triplanar {
            luma: plane("zgui.video.luma", luma, width, height),
            cb: plane("zgui.video.cb", cb, chroma_width, chroma_height),
            cr: plane("zgui.video.cr", cr, chroma_width, chroma_height),
        }
    }

    /// Uploads 8-bit NV12 planes from memory: full-size luma, then interleaved Cb and Cr at half
    /// size in each direction, rounded up.
    ///
    /// Each plane is a byte slice and the distance in bytes between its rows. Creates new
    /// textures on every call.
    pub fn upload_nv12(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        luma: (&[u8], u32),
        chroma: (&[u8], u32),
    ) -> Self {
        Self::Biplanar {
            luma: upload(
                device,
                queue,
                "zgui.video.luma",
                wgpu::TextureFormat::R8Unorm,
                (width, height),
                luma.0,
                luma.1,
            ),
            chroma: upload(
                device,
                queue,
                "zgui.video.chroma",
                wgpu::TextureFormat::Rg8Unorm,
                (width.div_ceil(2), height.div_ceil(2)),
                chroma.0,
                chroma.1,
            ),
        }
    }
}

/// Creates one sampled plane and writes `bytes` into it.
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    format: wgpu::TextureFormat,
    (width, height): (u32, u32),
    bytes: &[u8],
    stride: u32,
) -> Arc<wgpu::Texture> {
    let size = wgpu::Extent3d {
        width: width.max(1),
        height: height.max(1),
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
        bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(stride),
            rows_per_image: None,
        },
        size,
    );
    Arc::new(texture)
}

/// One frame of video, ready for [`SurfaceHandle::present_video`](crate::SurfaceHandle).
///
/// zgui converts the planes to colour on its device during the frame that shows them. A frame
/// presented while an earlier one waits replaces it unconverted.
pub struct VideoFrame {
    /// The samples.
    pub(crate) planes: Planes,
    /// What the samples mean.
    pub(crate) color: ColorSpace,
    /// The visible part of the luma plane, from its top-left corner.
    pub(crate) visible: Option<(u32, u32)>,
    /// What the producer keeps alive until the device finished reading the planes.
    pub(crate) guard: Option<Box<dyn Any + Send>>,
}

impl VideoFrame {
    /// A frame of `planes` whose samples mean `color`.
    pub fn new(planes: Planes, color: ColorSpace) -> Self {
        Self {
            planes,
            color,
            visible: None,
            guard: None,
        }
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
}

impl std::fmt::Debug for VideoFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VideoFrame")
            .field("planes", &self.planes)
            .field("color", &self.color)
            .field("size", &self.size())
            .field("guarded", &self.guard.is_some())
            .finish()
    }
}
