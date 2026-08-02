//! What every test on a real device needs.
//!
//! Shared by every test binary in this crate, and no binary uses all of it, so the module carries
//! one allowance for the parts a given binary does not reach rather than one attribute per item.

#![allow(
    dead_code,
    reason = "one support module serves several test binaries, and none of them uses all of it"
)]

use std::sync::{Mutex, MutexGuard, OnceLock};

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Device, DevicePx, Point, Rect, Scale, Size, Vec2};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_vector_coverage::CoverageRaster;
use zgui_render_vector_vello::VelloRaster;
use zgui_render_wgpu::frame::vector::VectorSource;
use zgui_render_wgpu::{Builder, Pixels, WgpuRenderer, wgpu};
use zgui_scene::{ClipId, ClipLink, PaintRef, Quad, Scene, VectorId, VectorItem};

/// The extent every pixel assertion here runs at.
pub(crate) const SIDE: i32 = 128;

/// Serialises every test in a binary onto the device.
///
/// A program has one graphics device; these tests are the only thing that would ever ask for
/// several at once, on several threads, which is not what any driver is asked to do in production.
pub(crate) fn device_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let lock = LOCK.get_or_init(|| Mutex::new(()));
    lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A renderer with a rasteriser attached, and the device lock it holds.
pub(crate) struct Harness {
    /// The renderer.
    pub(crate) renderer: WgpuRenderer,
    /// Held so that no two tests are on the device at once.
    _guard: MutexGuard<'static, ()>,
}

impl std::ops::Deref for Harness {
    type Target = WgpuRenderer;

    fn deref(&self) -> &WgpuRenderer {
        &self.renderer
    }
}

impl std::ops::DerefMut for Harness {
    fn deref_mut(&mut self) -> &mut WgpuRenderer {
        &mut self.renderer
    }
}

/// Which rasteriser a harness is built with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Which {
    /// The compute-shader path renderer.
    Vello,
    /// The fallback, which needs no compute shaders.
    Coverage,
}

/// A renderer at `side` square with `which` rasteriser attached, or `None` with no usable device.
///
/// Returning `None` rather than failing is deliberate, and every caller says so out loud when it
/// skips: a silent green run on a machine with no adapter must be impossible to mistake for a pass.
pub(crate) fn harness_at(side: i32, which: Which) -> Option<Harness> {
    let guard = device_lock();
    let target = RenderTarget::new(Size::new(side, side), Scale::new(1.0));
    let mut renderer =
        match Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false) {
            Ok(renderer) => renderer,
            Err(failure) => {
                eprintln!("skipped: no usable graphics device ({failure})");
                for candidate in &failure.candidates {
                    eprintln!("    {}: {}", candidate.name, candidate.reason);
                }
                return None;
            }
        };
    let raster = raster(&renderer, side, which)?;
    renderer.set_vector_raster(raster);
    Some(Harness {
        renderer,
        _guard: guard,
    })
}

/// A renderer at the usual extent.
pub(crate) fn harness(which: Which) -> Option<Harness> {
    harness_at(SIDE, which)
}

/// Builds one rasteriser of the requested kind.
fn raster(renderer: &WgpuRenderer, side: i32, which: Which) -> Option<Box<dyn VectorSource>> {
    let gpu = renderer.gpu();
    let extent = side.max(1) as u32;
    match which {
        Which::Vello => match VelloRaster::new(gpu, extent, extent) {
            Ok(raster) => Some(Box::new(raster)),
            Err(failure) => {
                eprintln!("skipped: this device runs no path renderer ({failure})");
                None
            }
        },
        Which::Coverage => Some(Box::new(CoverageRaster::new(gpu, extent, extent))),
    }
}

/// What a rasteriser was asked to do, and how long it spent doing it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Work {
    /// How many times the scratch was cleared.
    pub(crate) clears: u32,
    /// How many times a frame was rasterised.
    pub(crate) preparations: u32,
    /// How many passes were planned across those frames.
    pub(crate) passes: u32,
    /// Nanoseconds spent inside the rasteriser.
    pub(crate) nanoseconds: u128,
}

/// A rasteriser that records what it was asked to do.
///
/// It is what turns "this frame ran no rasterisation work" into something assertable. Asserting it
/// from the outside — a counter, a memory figure — would pass for the wrong reason, because a
/// rasteriser holds its scratch whether or not a frame used it.
pub(crate) struct Counting {
    /// The real one.
    inner: Box<dyn VectorSource>,
    /// What it has been asked to do, shared so a test can read it while the renderer holds the
    /// rasteriser.
    work: std::sync::Arc<Mutex<Work>>,
}

impl Counting {
    /// Wraps `inner`, and hands back the record it will write into.
    pub(crate) fn wrapping(
        inner: Box<dyn VectorSource>,
    ) -> (Box<dyn VectorSource>, std::sync::Arc<Mutex<Work>>) {
        let work = std::sync::Arc::new(Mutex::new(Work::default()));
        let counting = Self {
            inner,
            work: std::sync::Arc::clone(&work),
        };
        (Box::new(counting), work)
    }
}

impl zgui_render::VectorRaster for Counting {
    fn plan(&mut self, passes: &zgui_scene::ScenePassPlan) -> zgui_render::VectorPlan {
        let plan = self.inner.plan(passes);
        self.work
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .passes += plan.len() as u32;
        plan
    }

    fn clear_targets(&mut self, plan: &zgui_render::VectorPlan) {
        self.work
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .clears += 1;
        let started = std::time::Instant::now();
        self.inner.clear_targets(plan);
        let elapsed = started.elapsed().as_nanos();
        self.work
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .nanoseconds += elapsed;
    }

    fn prepare(
        &mut self,
        frame: &mut zgui_render::VectorFrame<'_>,
    ) -> Result<(), zgui_render::VectorError> {
        self.work
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .preparations += 1;
        let started = std::time::Instant::now();
        let outcome = self.inner.prepare(frame);
        let elapsed = started.elapsed().as_nanos();
        self.work
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .nanoseconds += elapsed;
        outcome
    }

    fn memory(&self) -> zgui_render::MemoryReport {
        // Deliberately not timed. Reporting what is held is not rasterisation work, and a frame
        // asks for it whether or not it rasterised anything — so counting it would make "zero time
        // inside the rasteriser" unassertable.
        self.inner.memory()
    }
}

impl VectorSource for Counting {
    fn view(&self, target: zgui_render::VectorTarget) -> Option<&wgpu::TextureView> {
        self.inner.view(target)
    }
}

/// Two renderers on one device, one per rasteriser, under one hold of the lock.
///
/// Asking for two through [`harness`] would wait for a lock the first is holding, and the seam test
/// needs the same scene through both implementations on the same device.
pub(crate) fn both() -> Option<(Harness, WgpuRenderer)> {
    let mut first = harness(Which::Vello)?;
    let target = RenderTarget::new(Size::new(SIDE, SIDE), Scale::new(1.0));
    let mut second = Builder::new()
        .offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false)
        .ok()?;
    let raster = raster(&second, SIDE, Which::Coverage)?;
    second.set_vector_raster(raster);
    let _ = &mut first;
    Some((first, second))
}

/// Two renderers at `side` square with the same rasteriser, under one hold of the lock.
///
/// What a damage assertion needs: one renderer is given the rectangles that changed and the other
/// repaints the whole surface, and the two surfaces are compared. They have to be two renderers
/// rather than one drawn twice, because the whole question is what the *retained* surface holds.
pub(crate) fn twins(side: i32, which: Which) -> Option<(Harness, WgpuRenderer)> {
    let scissored = harness_at(side, which)?;
    let target = RenderTarget::new(Size::new(side, side), Scale::new(1.0));
    let mut whole = Builder::new()
        .offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false)
        .ok()?;
    let raster = raster(&whole, side, which)?;
    whole.set_vector_raster(raster);
    Some((scissored, whole))
}

/// Where two surfaces of `side` square differ, and by how much on the widest channel.
pub(crate) fn difference(side: i32, one: &Pixels, two: &Pixels) -> Option<(i32, i32, i32)> {
    let mut worst: Option<(i32, i32, i32)> = None;
    for y in 0..side {
        for x in 0..side {
            let (left, right) = (one.rgba(x, y), two.rgba(x, y));
            let delta = (0..4)
                .map(|channel| (i32::from(left[channel]) - i32::from(right[channel])).abs())
                .max()
                .unwrap_or(0);
            if delta > 0 && worst.is_none_or(|(_, _, held)| delta > held) {
                worst = Some((x, y, delta));
            }
        }
    }
    worst
}

/// A rectangle in device pixels, grown outwards to whole ones with a pixel of slack.
pub(crate) fn whole_pixels(bounds: Rect<DevicePx, Device>) -> Rect<i32, Device> {
    Rect::from_corners(
        Point::new(
            bounds.left().0.floor() as i32 - 1,
            bounds.top().0.floor() as i32 - 1,
        ),
        Point::new(
            bounds.right().0.ceil() as i32 + 1,
            bounds.bottom().0.ceil() as i32 + 1,
        ),
    )
}

/// A harness whose rasteriser records what it was asked to do.
pub(crate) fn counting_harness(which: Which) -> Option<(Harness, std::sync::Arc<Mutex<Work>>)> {
    let guard = device_lock();
    let target = RenderTarget::new(Size::new(SIDE, SIDE), Scale::new(1.0));
    let mut renderer =
        match Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false) {
            Ok(renderer) => renderer,
            Err(failure) => {
                eprintln!("skipped: no usable graphics device ({failure})");
                return None;
            }
        };
    let inner = raster(&renderer, SIDE, which)?;
    let (counting, work) = Counting::wrapping(inner);
    renderer.set_vector_raster(counting);
    Some((
        Harness {
            renderer,
            _guard: guard,
        },
        work,
    ))
}

/// A rectangle in device pixels.
pub(crate) fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// A rectangular path.
pub(crate) fn path(bounds: Rect<DevicePx, Device>) -> std::sync::Arc<kurbo::BezPath> {
    use kurbo::Shape as _;
    std::sync::Arc::new(
        kurbo::Rect::new(
            f64::from(bounds.left().0),
            f64::from(bounds.top().0),
            f64::from(bounds.right().0),
            f64::from(bounds.bottom().0),
        )
        .to_path(0.01),
    )
}

/// A circular path.
pub(crate) fn circle(x: f32, y: f32, radius: f32) -> std::sync::Arc<kurbo::BezPath> {
    use kurbo::Shape as _;
    std::sync::Arc::new(
        kurbo::Circle::new((f64::from(x), f64::from(y)), f64::from(radius)).to_path(0.01),
    )
}

/// A scene over the usual extent.
pub(crate) fn scene_at(side: i32) -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(side, side));
    scene
}

/// A scene over the usual extent.
pub(crate) fn scene() -> Scene {
    scene_at(SIDE)
}

/// A solid paint, interned.
pub(crate) fn solid(scene: &mut Scene, color: Color) -> PaintRef {
    PaintRef::solid(scene.paints.solid(color))
}

/// An opaque colour from three bytes.
pub(crate) fn opaque(red: u8, green: u8, blue: u8) -> Color {
    Color::srgb_u8(red, green, blue, 255)
}

/// Pushes a filled quad.
pub(crate) fn quad(scene: &mut Scene, bounds: Rect<DevicePx, Device>, color: Color) {
    let fill = solid(scene, color);
    scene.push_quad(Quad::filled(bounds, fill));
}

/// Pushes a filled vector item.
pub(crate) fn vector(
    scene: &mut Scene,
    id: u32,
    geometry: std::sync::Arc<kurbo::BezPath>,
    color: Color,
    clip: ClipId,
) {
    let fill = solid(scene, color);
    scene.push_vector(VectorItem::filled(VectorId(id), geometry, fill).clipped(clip));
}

/// A rounded clip chain of one link.
pub(crate) fn rounded(
    scene: &mut Scene,
    parent: ClipId,
    bounds: Rect<DevicePx, Device>,
    radius: Vec2<DevicePx>,
) -> ClipId {
    scene.clips.push(parent, ClipLink::rounded(bounds, radius))
}

/// Draws `scene` fully and reads back what was presented.
pub(crate) fn present(renderer: &mut WgpuRenderer, scene: &Scene) -> Pixels {
    let outcome = renderer.draw(scene, &DamageSet::full());
    assert!(
        outcome.stats().is_some(),
        "a frame composed into a texture always reaches it, but this one reported {outcome:?}"
    );
    renderer
        .read_presented()
        .expect("a stand-in surface can be read back")
}
