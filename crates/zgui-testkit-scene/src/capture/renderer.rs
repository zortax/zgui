//! The renderer itself.

use zgui_bits::DamageSet;
use zgui_render::{
    ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer, TextureHandle,
};
use zgui_scene::Scene;

use crate::capture::external::Externals;
use crate::transcript::{self, Transcript};

/// A [`Renderer`] that records the display list as text and puts no pixels anywhere.
///
/// Every frame it is handed is transcribed and kept, so a test reads back exactly what the paint
/// stage produced — in draw order, with paints, clips and transforms resolved.
///
/// ```
/// use zgui_bits::DamageSet;
/// use zgui_color::Color;
/// use zgui_geom::{Device, DevicePx, Point, Rect, Size};
/// use zgui_render::Renderer;
/// use zgui_scene::{PaintRef, Quad, Scene};
/// use zgui_testkit_scene::CaptureRenderer;
///
/// let mut scene = Scene::new();
/// scene.begin_frame(Size::new(64, 64));
/// let fill = PaintRef::solid(scene.paints.solid(Color::srgb(1.0, 0.0, 0.0, 1.0)));
/// scene.push_quad(Quad::filled(
///     Rect::new(
///         Point::new(DevicePx(0.0), DevicePx(0.0)),
///         Size::new(DevicePx(8.0), DevicePx(8.0)),
///     ),
///     fill,
/// ));
/// scene.finish(&DamageSet::full());
///
/// let mut renderer = CaptureRenderer::new();
/// renderer.draw(&scene, &DamageSet::full());
///
/// let transcript = renderer.transcript().expect("a frame was drawn").to_string();
/// assert!(transcript.contains("quad order=1 bounds=rect(0, 0, 8, 8) fill=solid srgb(1, 0, 0, 1)"));
/// ```
///
/// # Why it always reports a presented frame
///
/// Every way a real frame fails to reach the screen is a property of a surface, and this renderer
/// has none: there is no swap chain to be outdated, no compositor to time out, and nothing to be
/// occluded by. Reporting anything else would be inventing a failure a test could then be written
/// against, and the failure would live only in the test double.
#[derive(Debug, Default)]
pub struct CaptureRenderer {
    /// The surface it was pointed at, if any.
    target: Option<RenderTarget>,
    /// Textures it was handed.
    externals: Externals,
    /// The most recent frame's transcript.
    last: Option<Transcript>,
    /// How many frames it has been asked to draw.
    frames: u64,
    /// Where its atlas tiles go: plain byte vectors, so the upload path runs for real.
    atlas: zgui_atlas::MemorySink,
}

impl CaptureRenderer {
    /// A renderer with nothing configured and nothing recorded.
    pub fn new() -> Self {
        Self::default()
    }

    /// The most recent frame's transcript, or `None` before any frame was drawn.
    pub fn transcript(&self) -> Option<&Transcript> {
        self.last.as_ref()
    }

    /// How many frames have been drawn.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// The external textures currently registered.
    pub fn externals(&self) -> &Externals {
        &self.externals
    }

    /// The atlas textures, for a test asking what a frame actually uploaded.
    pub fn atlas(&self) -> &zgui_atlas::MemorySink {
        &self.atlas
    }

    /// Forgets the recorded transcript and the frame count.
    pub fn reset(&mut self) {
        self.last = None;
        self.frames = 0;
    }
}

impl Renderer for CaptureRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        // The least capable device worth supporting, so that anything above chooses the fallback
        // path a machine without dual-source blending or compute would take. A capture renderer
        // claiming capabilities it does not exercise would silently stop those paths being tested
        // anywhere at all.
        RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        self.frames += 1;
        self.last = Some(transcript::of(scene, damage));
        FrameOutcome::Presented(FrameStats {
            // The scene planned these before any renderer saw it, so the count is the scene's
            // property and means the same thing here as on a device.
            vector_passes: scene.pass_plan().passes.len() as u32,
            // Nothing was submitted, nothing was scissored and nothing was uploaded. These read
            // zero because there was no renderer, which is why `crate::counters` refuses an
            // assertion on their counters rather than letting one pass.
            draw_calls: 0,
            damage_px: 0,
            bytes_uploaded: 0,
            memory: MemoryReport::ZERO,
        })
    }

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        &mut self.atlas
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        self.externals.register(texture)
    }

    fn release_external(&mut self, handle: TextureHandle) {
        self.externals.release(handle);
    }

    fn memory(&self) -> MemoryReport {
        // Nothing is held on a device, because there is no device.
        MemoryReport::ZERO
    }
}
