//! What every test on a real device needs.

use std::sync::{Mutex, MutexGuard, OnceLock};

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Point, Rect, Scale, Size};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Builder, Pixels, WgpuRenderer, wgpu};

/// The extent every pixel assertion here runs at.
pub(crate) const SIDE: i32 = 128;

/// Serialises every test in a binary onto the device.
///
/// A program has one graphics device, and these tests are the only thing in the workspace that
/// would ever have several: one per test, created and destroyed on several threads at once. That
/// is not what the code under test does and not what any driver is asked to do in production, and
/// on a real driver it does not survive. Holding this for the length of a test keeps the process
/// to one device at a time.
pub(crate) fn device_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    // A test that failed while holding it poisoned it; the next test still wants the device.
    lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A renderer, and the device lock it holds for as long as it lives.
pub(crate) struct TestRenderer {
    /// The renderer.
    renderer: WgpuRenderer,
    /// Held so that no two tests are on the device at once.
    _guard: MutexGuard<'static, ()>,
}

impl std::ops::Deref for TestRenderer {
    type Target = WgpuRenderer;

    fn deref(&self) -> &WgpuRenderer {
        &self.renderer
    }
}

impl std::ops::DerefMut for TestRenderer {
    fn deref_mut(&mut self) -> &mut WgpuRenderer {
        &mut self.renderer
    }
}

/// A renderer presenting to a texture of `format`, or `None` when this machine has no device.
///
/// Returning `None` rather than failing is deliberate: these tests assert what a device does, and
/// a machine without one has nothing to say about it. Every one of them says so out loud when it
/// skips, so a silent green run on a machine with no adapter is impossible to mistake for a pass.
pub(crate) fn renderer(
    format: wgpu::TextureFormat,
    mutable_texture_formats: bool,
) -> Option<TestRenderer> {
    let guard = device_lock();
    let target = RenderTarget::new(Size::new(SIDE, SIDE), Scale::new(1.0));
    match Builder::new().offscreen(target, format, mutable_texture_formats) {
        Ok(renderer) => Some(TestRenderer {
            renderer,
            _guard: guard,
        }),
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            for candidate in &failure.candidates {
                eprintln!("    {}: {}", candidate.name, candidate.reason);
            }
            None
        }
    }
}

/// A renderer presenting to an unencoded texture, which is what most assertions want.
pub(crate) fn plain_renderer() -> Option<TestRenderer> {
    renderer(wgpu::TextureFormat::Bgra8Unorm, false)
}

/// Two renderers on one device, under one hold of the lock.
///
/// Asking for two through [`plain_renderer`] would wait for a lock the first is holding, so a
/// comparison between two renderings needs its own way in. The two are otherwise ordinary: same
/// device, same formats, same everything a single one gets.
#[allow(
    dead_code,
    reason = "not every test binary that shares this module compares two renderings"
)]
pub(crate) fn renderer_pair() -> Option<(TestRenderer, WgpuRenderer)> {
    let first = plain_renderer()?;
    let target = RenderTarget::new(Size::new(SIDE, SIDE), Scale::new(1.0));
    let second = Builder::new()
        .offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false)
        .ok()?;
    Some((first, second))
}

/// A rectangle in device pixels.
#[allow(
    dead_code,
    reason = "not every test binary that shares this module names a rectangle"
)]
pub(crate) fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// An opaque colour from three bytes.
#[allow(
    dead_code,
    reason = "not every test binary that shares this module needs a colour"
)]
pub(crate) fn opaque(red: u8, green: u8, blue: u8) -> Color {
    Color::srgb_u8(red, green, blue, 255)
}

/// Draws `scene` and reads back what was presented.
///
///
/// # Panics
///
/// Panics if the frame did not reach the target, which for a stand-in surface cannot happen and
/// would mean the renderer stopped composing.
#[allow(
    dead_code,
    reason = "not every test binary that shares this module draws a whole frame"
)]
pub(crate) fn present(renderer: &mut WgpuRenderer, scene: &zgui_scene::Scene) -> Pixels {
    let outcome = renderer.draw(scene, &DamageSet::full());
    assert!(
        outcome.stats().is_some(),
        "a frame composed into a texture always reaches it, but this one reported {outcome:?}"
    );
    renderer
        .read_presented()
        .expect("a stand-in surface can be read back")
}
