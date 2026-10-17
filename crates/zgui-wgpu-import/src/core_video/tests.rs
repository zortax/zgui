//! Importing pixel buffers made here the way a decoder makes them.

use std::ptr::NonNull;
use std::sync::{Arc, Mutex, MutexGuard};

use objc2_core_foundation::{CFBoolean, CFDictionary, CFRetained, CFString, CFType};
use objc2_core_video::{
    CVAttachmentMode, CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddressOfPlane,
    CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress, kCVImageBufferChromaLocation_Center,
    kCVImageBufferChromaLocationTopFieldKey, kCVImageBufferColorPrimaries_ITU_R_2020,
    kCVImageBufferColorPrimariesKey, kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ,
    kCVImageBufferTransferFunctionKey, kCVImageBufferYCbCrMatrix_ITU_R_2020,
    kCVImageBufferYCbCrMatrixKey, kCVPixelBufferIOSurfacePropertiesKey,
    kCVPixelBufferMetalCompatibilityKey, kCVPixelFormatType_32BGRA,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange, kCVReturnSuccess,
};
use zgui_geom::{Scale, Size};
use zgui_render::RenderTarget;
use zgui_render_wgpu::{Builder, Gpu, wgpu};
use zgui_wgpu::{ChromaSiting, ColorRange, ColorSpace, Planes};

use super::CoreVideoImporter;
use crate::ImportError;

/// One device for the whole binary, held for the length of a test.
fn device() -> Option<(Arc<Gpu>, MutexGuard<'static, ()>)> {
    static LOCK: Mutex<()> = Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let target = RenderTarget::new(Size::new(8, 8), Scale::new(1.0));
    match Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false) {
        Ok(renderer) => Some((Arc::clone(renderer.gpu()), guard)),
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            None
        }
    }
}

/// An IOSurface-backed pixel buffer of `format`, as a decoder allocates one.
fn pixel_buffer(format: u32, width: usize, height: usize) -> CFRetained<CVPixelBuffer> {
    let empty = CFDictionary::<CFString, CFType>::empty();
    // SAFETY: CoreFoundation's own constants, valid for the process.
    let (surface, metal, yes) = unsafe {
        (
            kCVPixelBufferIOSurfacePropertiesKey,
            kCVPixelBufferMetalCompatibilityKey,
            CFBoolean::new(true),
        )
    };
    let attributes =
        CFDictionary::<CFString, CFType>::from_slices(&[surface, metal], &[&empty, yes]);
    let mut out = std::ptr::null_mut();
    // SAFETY: `out` is a valid out pointer and `attributes` is a CFString-keyed dictionary.
    let status = unsafe {
        CVPixelBufferCreate(
            None,
            width,
            height,
            format,
            Some(attributes.as_opaque()),
            NonNull::from(&mut out),
        )
    };
    assert_eq!(status, kCVReturnSuccess);
    // SAFETY: CoreVideo created the buffer with a reference this side now owns.
    unsafe { CFRetained::from_raw(NonNull::new(out).expect("a buffer")) }
}

/// Writes `value` into every sample of plane `index`, `channels` bytes each.
fn fill(buffer: &CVPixelBuffer, index: usize, value: &[u8]) {
    // SAFETY: the base address is valid between the lock and the unlock, and each row holds
    // `bytes_per_row` bytes of which the plane's width in samples are written.
    unsafe {
        assert_eq!(
            CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags(0)),
            kCVReturnSuccess
        );
        let base = CVPixelBufferGetBaseAddressOfPlane(buffer, index).cast::<u8>();
        let stride = CVPixelBufferGetBytesPerRowOfPlane(buffer, index);
        let width = objc2_core_video::CVPixelBufferGetWidthOfPlane(buffer, index);
        let height = objc2_core_video::CVPixelBufferGetHeightOfPlane(buffer, index);
        for row in 0..height {
            let start = base.add(row * stride);
            for column in 0..width {
                let at = start.add(column * value.len());
                std::ptr::copy_nonoverlapping(value.as_ptr(), at, value.len());
            }
        }
        CVPixelBufferUnlockBaseAddress(buffer, CVPixelBufferLockFlags(0));
    }
}

/// Attaches `value` under `key`, as a decoder states a frame's colour.
fn attach(buffer: &CVPixelBuffer, key: &CFString, value: &CFString) {
    // SAFETY: both are CFStrings, and the mode is one CoreVideo defines.
    unsafe { buffer.set_attachment(key, value, CVAttachmentMode::ShouldPropagate) };
}

/// Reads plane `texture` back, one byte per sample channel.
fn read(gpu: &Gpu, texture: &wgpu::Texture) -> Vec<u8> {
    let texel = texture
        .format()
        .block_copy_size(None)
        .expect("a colour format");
    let row = (texture.width() * texel).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * texture.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    gpu.queue().submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    gpu.wait();
    let bytes = buffer.slice(..).get_mapped_range();
    bytes[..(texture.width() * texel) as usize].to_vec()
}

#[test]
fn a_video_range_buffer_imports_as_two_planes_over_its_own_samples() {
    let Some((gpu, _held)) = device() else { return };
    let buffer = pixel_buffer(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, 16, 4);
    fill(&buffer, 0, &[63]);
    fill(&buffer, 1, &[102, 240]);
    let importer = CoreVideoImporter::new(gpu.device()).expect("a Metal device");
    let frame = importer.import(gpu.device(), &buffer).expect("an import");

    let Planes::Biplanar { luma, chroma } = frame.planes() else {
        panic!("a bi-planar buffer imports as two planes");
    };
    assert_eq!((luma.width(), luma.height()), (16, 4));
    assert_eq!((chroma.width(), chroma.height()), (8, 2));
    assert_eq!(read(&gpu, luma), vec![63; 16]);
    assert_eq!(read(&gpu, chroma), [102, 240].repeat(8));
    assert_eq!(
        frame.color(),
        ColorSpace::BT709,
        "no attachments read as BT.709"
    );
}

#[test]
fn the_attachments_state_the_colour_space_and_siting() {
    let Some((gpu, _held)) = device() else { return };
    let buffer = pixel_buffer(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, 16, 4);
    // SAFETY: CoreVideo's own constant strings.
    unsafe {
        attach(
            &buffer,
            kCVImageBufferYCbCrMatrixKey,
            kCVImageBufferYCbCrMatrix_ITU_R_2020,
        );
        attach(
            &buffer,
            kCVImageBufferColorPrimariesKey,
            kCVImageBufferColorPrimaries_ITU_R_2020,
        );
        attach(
            &buffer,
            kCVImageBufferTransferFunctionKey,
            kCVImageBufferTransferFunction_SMPTE_ST_2084_PQ,
        );
        attach(
            &buffer,
            kCVImageBufferChromaLocationTopFieldKey,
            kCVImageBufferChromaLocation_Center,
        );
    }
    let importer = CoreVideoImporter::new(gpu.device()).expect("a Metal device");
    let frame = importer.import(gpu.device(), &buffer).expect("an import");
    assert_eq!(frame.color(), ColorSpace::BT2100_PQ);
    assert_eq!(frame.color().range, ColorRange::Limited);
    assert!(format!("{frame:?}").contains(&format!("{:?}", ChromaSiting::Center)));
}

#[test]
fn an_rgb_buffer_is_refused_by_name() {
    let Some((gpu, _held)) = device() else { return };
    let buffer = pixel_buffer(kCVPixelFormatType_32BGRA, 16, 4);
    let importer = CoreVideoImporter::new(gpu.device()).expect("a Metal device");
    let refused = importer.import(gpu.device(), &buffer).err();
    assert_eq!(refused, Some(ImportError::Format("BGRA".to_owned())));
}

#[test]
fn the_guard_holds_the_buffer_until_the_frame_is_gone() {
    let Some((gpu, _held)) = device() else { return };
    let buffer = pixel_buffer(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, 16, 4);
    let before = buffer.retain_count();
    let importer = CoreVideoImporter::new(gpu.device()).expect("a Metal device");
    let frame = importer.import(gpu.device(), &buffer).expect("an import");
    assert!(
        buffer.retain_count() > before,
        "the frame holds a reference"
    );
    drop(frame);
    gpu.wait();
    assert_eq!(buffer.retain_count(), before, "and lets it go");
}

#[test]
fn a_ten_bit_buffer_imports_as_sixteen_bit_planes_with_its_codes_high() {
    let Some((gpu, _held)) = device() else { return };
    let buffer = pixel_buffer(kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange, 16, 4);
    let sample = |code: u16| (code << 6).to_ne_bytes();
    fill(&buffer, 0, &sample(250));
    fill(&buffer, 1, &[sample(409), sample(960)].concat());
    let importer = CoreVideoImporter::new(gpu.device()).expect("a Metal device");
    let frame = match importer.import(gpu.device(), &buffer) {
        Err(ImportError::Missing(what)) => {
            eprintln!("skipped: the device lacks {what}");
            return;
        }
        other => other.expect("an import"),
    };
    let Planes::Biplanar { luma, .. } = frame.planes() else {
        panic!("a bi-planar buffer imports as two planes");
    };
    assert_eq!(luma.format(), wgpu::TextureFormat::R16Unorm);
    assert_eq!(read(&gpu, luma), sample(250).repeat(16));
    assert!(format!("{frame:?}").contains("High"), "{frame:?}");
}
