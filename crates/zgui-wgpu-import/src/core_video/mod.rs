//! macOS: CoreVideo pixel buffers as Metal textures over the same IOSurface.
//!
//! VideoToolbox, AVFoundation and ScreenCaptureKit hand out frames as IOSurface-backed
//! `CVPixelBuffer`s. A `CVMetalTextureCache` makes a Metal texture over each plane without a
//! copy, and the texture reaches wgpu through its hal. CoreVideo requires the `CVMetalTexture`
//! wrapper to stay alive until the GPU has finished with the texture, so the frame's guard holds
//! it beside the pixel buffer.

mod metadata;

use std::ptr::NonNull;
use std::sync::Arc;

use objc2_core_foundation::CFRetained;
use objc2_core_video::{
    CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVPixelBuffer,
    CVPixelBufferGetHeightOfPlane, CVPixelBufferGetPixelFormatType, CVPixelBufferGetPlaneCount,
    CVPixelBufferGetWidthOfPlane, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange, kCVReturnSuccess,
};
use objc2_metal::{MTLPixelFormat, MTLTextureType};
use zgui_render_wgpu::wgpu;
use zgui_wgpu::{ColorRange, Planes, SampleDepth, VideoFrame};

use crate::ImportError;

/// Imports CoreVideo pixel buffers on one Metal device.
pub struct CoreVideoImporter {
    /// The cache that makes Metal textures over pixel buffer planes.
    cache: CFRetained<CVMetalTextureCache>,
}

/// What one supported pixel format is.
struct Layout {
    /// The wgpu and Metal formats of the luma and chroma planes.
    planes: [(wgpu::TextureFormat, MTLPixelFormat); 2],
    /// The code depth, where it is deeper than the planes' own.
    depth: Option<SampleDepth>,
    /// The range the format states.
    range: ColorRange,
}

/// `420v`: 8-bit bi-planar 4:2:0, limited range.
const EIGHT_LIMITED: u32 = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange;
/// `420f`: 8-bit bi-planar 4:2:0, full range.
const EIGHT_FULL: u32 = kCVPixelFormatType_420YpCbCr8BiPlanarFullRange;
/// `x420`: 10-bit bi-planar 4:2:0 in the high bits of 16, limited range.
const TEN_LIMITED: u32 = kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange;
/// `xf20`: 10-bit bi-planar 4:2:0 in the high bits of 16, full range.
const TEN_FULL: u32 = kCVPixelFormatType_420YpCbCr10BiPlanarFullRange;

/// The layout of `format`, if it is one this importer reads.
fn layout(format: u32) -> Option<Layout> {
    let eight = [
        (wgpu::TextureFormat::R8Unorm, MTLPixelFormat::R8Unorm),
        (wgpu::TextureFormat::Rg8Unorm, MTLPixelFormat::RG8Unorm),
    ];
    let sixteen = [
        (wgpu::TextureFormat::R16Unorm, MTLPixelFormat::R16Unorm),
        (wgpu::TextureFormat::Rg16Unorm, MTLPixelFormat::RG16Unorm),
    ];
    let (planes, depth, range) = match format {
        EIGHT_LIMITED => (eight, None, ColorRange::Limited),
        EIGHT_FULL => (eight, None, ColorRange::Full),
        TEN_LIMITED => (sixteen, Some(SampleDepth::P010), ColorRange::Limited),
        TEN_FULL => (sixteen, Some(SampleDepth::P010), ColorRange::Full),
        _ => return None,
    };
    Some(Layout {
        planes,
        depth,
        range,
    })
}

/// A four-character code as text.
fn fourcc(code: u32) -> String {
    code.to_be_bytes()
        .iter()
        .map(|&byte| {
            if byte.is_ascii_graphic() {
                byte as char
            } else {
                '?'
            }
        })
        .collect()
}

/// What a frame keeps alive until the GPU has read its planes.
struct Held {
    /// The decoder's buffer.
    _buffer: CFRetained<CVPixelBuffer>,
    /// The Metal texture wrappers CoreVideo requires to outlive the GPU's reads.
    _planes: [CFRetained<CVMetalTexture>; 2],
}

// SAFETY: CoreVideo buffers are reference counted with atomic operations, and the planes are only
// read from here on. Releasing the last reference on another thread is what CoreVideo's own
// pools do when a frame leaves a decoder.
unsafe impl Send for Held {}

impl CoreVideoImporter {
    /// An importer for `device`.
    ///
    /// # Errors
    ///
    /// [`ImportError::Backend`] on a device that is not a Metal one, and
    /// [`ImportError::Platform`] when CoreVideo refuses a texture cache.
    pub fn new(device: &wgpu::Device) -> Result<Self, ImportError> {
        // SAFETY: the hal device is read for its Metal device and released before this returns.
        let hal =
            unsafe { device.as_hal::<wgpu::hal::api::Metal>() }.ok_or(ImportError::Backend)?;
        let mut cache = std::ptr::null_mut();
        // SAFETY: `cache` is a valid out pointer, and both attribute dictionaries are absent.
        let status = unsafe {
            CVMetalTextureCache::create(
                None,
                None,
                hal.raw_device(),
                None,
                NonNull::from(&mut cache),
            )
        };
        let cache = NonNull::new(cache)
            .filter(|_| status == kCVReturnSuccess)
            .ok_or_else(|| {
                ImportError::Platform(format!("CVMetalTextureCacheCreate returned {status}"))
            })?;
        // SAFETY: CoreVideo created the cache with a reference this side now owns.
        let cache = unsafe { CFRetained::from_raw(cache) };
        Ok(Self { cache })
    }

    /// Wraps `buffer` as a video frame on `device`, the device this importer was made for.
    ///
    /// Reads 8-bit (`420v`, `420f`) and 10-bit (`x420`, `xf20`) bi-planar 4:2:0 buffers. The
    /// buffer must be IOSurface-backed, which every buffer VideoToolbox decodes into is. The
    /// colour space and chroma siting are read from the buffer's attachments.
    ///
    /// # Errors
    ///
    /// [`ImportError::Format`] for any other pixel format, [`ImportError::Missing`] for 10-bit
    /// buffers on a device without 16-bit normalised textures, and [`ImportError::Platform`] when
    /// CoreVideo refuses a plane.
    pub fn import(
        &self,
        device: &wgpu::Device,
        buffer: &CVPixelBuffer,
    ) -> Result<VideoFrame, ImportError> {
        let format = CVPixelBufferGetPixelFormatType(buffer);
        let layout = layout(format)
            .filter(|_| CVPixelBufferGetPlaneCount(buffer) == 2)
            .ok_or_else(|| ImportError::Format(fourcc(format)))?;
        let needed = layout.planes[0].0.required_features();
        if !device.features().contains(needed) {
            return Err(ImportError::Missing("16-bit normalised textures"));
        }

        let (luma, luma_wrapper) = self.plane(device, buffer, 0, layout.planes[0])?;
        let (chroma, chroma_wrapper) = self.plane(device, buffer, 1, layout.planes[1])?;
        // Lets the cache recycle the wrappers of frames whose guards already let go.
        self.cache.flush(0);

        let color = metadata::color_space(buffer).with_range(layout.range);
        let held = Held {
            _buffer: CFRetained::from(buffer),
            _planes: [luma_wrapper, chroma_wrapper],
        };
        let mut frame = VideoFrame::new(Planes::Biplanar { luma, chroma }, color)
            .with_chroma_siting(metadata::chroma_siting(buffer))
            .with_guard(held);
        if let Some(depth) = layout.depth {
            frame = frame.with_depth(depth);
        }
        Ok(frame)
    }

    /// A texture over plane `index` of `buffer`, and the wrapper that keeps it valid.
    fn plane(
        &self,
        device: &wgpu::Device,
        buffer: &CVPixelBuffer,
        index: usize,
        (format, metal_format): (wgpu::TextureFormat, MTLPixelFormat),
    ) -> Result<(Arc<wgpu::Texture>, CFRetained<CVMetalTexture>), ImportError> {
        let width = CVPixelBufferGetWidthOfPlane(buffer, index);
        let height = CVPixelBufferGetHeightOfPlane(buffer, index);
        let mut wrapper = std::ptr::null_mut();
        // SAFETY: `wrapper` is a valid out pointer, and no texture attributes are passed.
        let status = unsafe {
            CVMetalTextureCache::create_texture_from_image(
                None,
                &self.cache,
                buffer,
                None,
                metal_format,
                width,
                height,
                index,
                NonNull::from(&mut wrapper),
            )
        };
        let wrapper = NonNull::new(wrapper)
            .filter(|_| status == kCVReturnSuccess)
            .ok_or_else(|| {
                ImportError::Platform(format!(
                    "plane {index}: CVMetalTextureCacheCreateTextureFromImage returned {status}; \
                     the buffer may not be IOSurface-backed"
                ))
            })?;
        // SAFETY: CoreVideo created the wrapper with a reference this side now owns.
        let wrapper = unsafe { CFRetained::from_raw(wrapper) };
        let raw = CVMetalTextureGetTexture(&wrapper)
            .ok_or_else(|| ImportError::Platform(format!("plane {index} has no Metal texture")))?;

        let size = wgpu::Extent3d {
            width: width as u32,
            height: height as u32,
            depth_or_array_layers: 1,
        };
        // SAFETY: `raw` is a live 2D Metal texture of `metal_format`, which is the Metal spelling
        // of `format`, with one layer, one mip level and the plane's extent.
        let hal = unsafe {
            wgpu::hal::metal::Device::texture_from_raw(
                raw,
                format,
                MTLTextureType::Type2D,
                1,
                1,
                wgpu::hal::CopyExtent {
                    width: size.width,
                    height: size.height,
                    depth: 1,
                },
            )
        };
        // SAFETY: the texture was made on this device's Metal device, matches the descriptor, and
        // holds the decoder's initialised samples.
        let texture = unsafe {
            device.create_texture_from_hal::<wgpu::hal::api::Metal>(
                hal,
                &wgpu::TextureDescriptor {
                    label: Some("zgui.video.core_video"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                },
            )
        };
        Ok((Arc::new(texture), wrapper))
    }
}

#[cfg(test)]
mod tests;
