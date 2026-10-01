//! A device and known pictures for the tests that need real planes.

use std::sync::{Arc, Mutex, MutexGuard};

use zgui_geom::{Scale, Size};
use zgui_render::RenderTarget;
use zgui_render_wgpu::{Builder, Gpu};

use super::{PlaneData, Planes, SampleSize};
use crate::wgpu;

/// BT.709 limited-range codes of pure red and pure blue.
pub(crate) const RED: [u8; 3] = [63, 102, 240];
pub(crate) const BLUE: [u8; 3] = [32, 240, 118];

/// One device for the whole binary, held for the length of a test.
pub(crate) fn device() -> Option<(Arc<Gpu>, MutexGuard<'static, ()>)> {
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

/// A tightly packed plane of `width` × `height` samples.
pub(crate) fn plane(bytes: &[u8], width: u32, height: u32) -> PlaneData<'_> {
    PlaneData {
        bytes,
        stride: bytes.len() as u32 / height,
        width,
        height,
    }
}

/// An 8×2 picture split into a left and a right colour, as 4:2:0 codes.
pub(crate) fn halves(left: [u8; 3], right: [u8; 3]) -> [Vec<u8>; 3] {
    let luma = (0..16).map(|i| if i % 8 < 4 { left[0] } else { right[0] });
    let chroma = |c: usize| vec![left[c], left[c], right[c], right[c]];
    [luma.collect(), chroma(1), chroma(2)]
}

/// An 8×2 picture of one colour's codes, as 4:2:0 planes.
pub(crate) fn solid(gpu: &Gpu, codes: [u8; 3]) -> Planes {
    upload(gpu, halves(codes, codes))
}

/// 8-bit 4:2:0 codes of an 8×2 picture, uploaded.
pub(crate) fn upload(gpu: &Gpu, [luma, cb, cr]: [Vec<u8>; 3]) -> Planes {
    Planes::upload_triplanar(
        gpu.device(),
        gpu.queue(),
        SampleSize::U8,
        [plane(&luma, 8, 2), plane(&cb, 4, 1), plane(&cr, 4, 1)],
    )
    .expect("8-bit planes are always supported")
}

/// An 8×2 picture: red on the left half, blue on the right, as three planes.
pub(crate) fn i420(gpu: &Gpu) -> Planes {
    upload(gpu, halves(RED, BLUE))
}

/// Reads an `Rgba8Unorm` or `Rgba16Float` texture back as RGBA in [0, 1], row by row.
pub(crate) fn read(gpu: &Gpu, texture: &wgpu::Texture) -> Vec<[f32; 4]> {
    let (width, height) = (texture.width(), texture.height());
    let texel = texture
        .format()
        .block_copy_size(None)
        .expect("a colour format");
    let row = (width * texel).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("zgui.video.test.read"),
        size: u64::from(row * height),
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
    let mut out = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let at = (y * row + x * texel) as usize;
            out.push(std::array::from_fn(|c| match texture.format() {
                wgpu::TextureFormat::Rgba8Unorm => f32::from(bytes[at + c]) / 255.0,
                wgpu::TextureFormat::Rgba16Float => half(u16::from_ne_bytes([
                    bytes[at + 2 * c],
                    bytes[at + 2 * c + 1],
                ])),
                other => panic!("no test readback for {other:?}"),
            }));
        }
    }
    out
}

/// An IEEE half-precision value, widened.
fn half(bits: u16) -> f32 {
    let sign = if bits >> 15 == 1 { -1.0 } else { 1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let mantissa = f32::from(bits & 0x3ff);
    sign * match exponent {
        0 => mantissa * 2f32.powi(-24),
        31 => f32::INFINITY,
        _ => (1.0 + mantissa / 1024.0) * 2f32.powi(exponent - 15),
    }
}
