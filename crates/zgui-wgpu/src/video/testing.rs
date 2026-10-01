//! A device and a known picture for the tests that need real planes.

use std::sync::{Arc, Mutex, MutexGuard};

use zgui_geom::{Scale, Size};
use zgui_render::RenderTarget;
use zgui_render_wgpu::{Builder, Gpu};

use super::Planes;
use crate::wgpu;

/// BT.709 limited-range samples of pure red and pure blue.
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

/// An 8×2 picture: red on the left half, blue on the right, as three planes.
pub(crate) fn i420(gpu: &Gpu) -> Planes {
    let luma: Vec<u8> = (0..16)
        .map(|i| if i % 8 < 4 { RED[0] } else { BLUE[0] })
        .collect();
    let cb = [RED[1], RED[1], BLUE[1], BLUE[1]];
    let cr = [RED[2], RED[2], BLUE[2], BLUE[2]];
    Planes::upload_i420(
        gpu.device(),
        gpu.queue(),
        8,
        2,
        (&luma, 8),
        (&cb, 4),
        (&cr, 4),
    )
}
