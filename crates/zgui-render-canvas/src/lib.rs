//! Draws zgui display lists with the browser's Canvas 2D API.
//!
//! Text is shaped and rasterized by zgui. This backend composites its atlas glyphs,
//! scene primitives and vector paths into a persistent HTML canvas.
//! See the crate README for the supported rendering subset.

#![forbid(unsafe_code)]
#![cfg(target_arch = "wasm32")]

mod atlas;
mod frame;
mod paint;
mod path;

use std::collections::BTreeSet;

use wasm_bindgen::{JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};
use zgui_atlas::TextureSink;
use zgui_bits::DamageSet;
use zgui_render::vector::{VectorBackend, VectorStatus};
use zgui_render::{
    ExternalTexture, FrameOutcome, FrameStats, MemoryReport, RenderCapabilities, RenderTarget,
    Renderer, SkipReason, TextureHandle,
};
use zgui_scene::Scene;

use crate::atlas::CanvasAtlas;

/// A browser canvas that receives the finished zgui scene.
///
/// Construct this before starting the platform event loop, then return it from
/// the application's renderer factory. The canvas must have no existing WebGPU
/// or WebGL context: a browser fixes its context type on first use.
pub struct CanvasRenderer {
    surface: Surface,
    target: Option<RenderTarget>,
    atlas: CanvasAtlas,
    warnings: BTreeSet<&'static str>,
    last_uploads: u64,
    frames: u64,
}

#[derive(Clone)]
pub(crate) struct Surface {
    pub canvas: HtmlCanvasElement,
    pub context: CanvasRenderingContext2d,
}

impl Surface {
    pub(crate) fn from_canvas(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let context = canvas
            .get_context("2d")?
            .ok_or_else(|| JsValue::from_str("Canvas 2D is unavailable"))?
            .dyn_into::<CanvasRenderingContext2d>()?;
        Ok(Self { canvas, context })
    }

    pub(crate) fn new(width: u32, height: u32) -> Result<Self, JsValue> {
        let document = web_sys::window()
            .and_then(|window| window.document())
            .ok_or_else(|| JsValue::from_str("Canvas rendering requires a browser document"))?;
        let canvas = document
            .create_element("canvas")?
            .dyn_into::<HtmlCanvasElement>()?;
        canvas.set_width(width);
        canvas.set_height(height);
        Self::from_canvas(canvas)
    }
}

impl CanvasRenderer {
    /// Opens a Canvas 2D context on `canvas`.
    pub fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        Ok(Self {
            surface: Surface::from_canvas(canvas)?,
            target: None,
            atlas: CanvasAtlas::default(),
            warnings: BTreeSet::new(),
            last_uploads: 0,
            frames: 0,
        })
    }

    /// The number of scenes successfully drawn by this renderer.
    pub fn frames(&self) -> u64 {
        self.frames
    }
}

impl Renderer for CanvasRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.surface
            .canvas
            .set_width(target.size.width.max(1) as u32);
        self.surface
            .canvas
            .set_height(target.size.height.max(1) as u32);
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        let Some(target) = self.target else {
            return FrameOutcome::Skipped(SkipReason::Unconfigured);
        };
        if damage.is_empty() {
            return FrameOutcome::Skipped(SkipReason::Undamaged);
        }
        let context = &self.surface.context;
        context.save();
        let result = frame::draw(
            self.surface.clone(),
            scene,
            damage,
            &mut self.atlas,
            &mut self.warnings,
        );
        context.restore();
        let calls = match result {
            Ok(calls) => calls,
            Err(error) => {
                web_sys::console::error_2(&JsValue::from_str("zgui Canvas 2D draw failed"), &error);
                return FrameOutcome::Skipped(SkipReason::DeviceUnavailable);
            }
        };
        self.frames += 1;
        let uploads = self.atlas.bytes_written();
        let stats = FrameStats {
            draw_calls: calls,
            vector_passes: scene.pass_plan().passes.len() as u32,
            damage_px: damage
                .area()
                .map_or(target.area(), |area| area.max(0) as u64),
            bytes_uploaded: uploads.saturating_sub(self.last_uploads),
            memory: self.memory(),
        };
        self.last_uploads = uploads;
        FrameOutcome::Presented(stats)
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        if self.warnings.insert("external GPU textures") {
            web_sys::console::warn_1(&JsValue::from_str(
                "zgui Canvas 2D: external GPU textures are unsupported",
            ));
        }
        texture.handle
    }

    fn release_external(&mut self, _handle: TextureHandle) {}

    fn memory(&self) -> MemoryReport {
        MemoryReport {
            targets: self.target.map_or(0, |target| target.area() * 4),
            atlases: self.atlas.bytes_held(),
            ..MemoryReport::ZERO
        }
    }

    fn vector_status(&self) -> VectorStatus {
        VectorStatus {
            backend: Some(VectorBackend::Other),
            initialized: true,
        }
    }

    fn release_cached_targets(&mut self) -> u64 {
        self.atlas.clear_images()
    }

    fn texture_sink(&mut self) -> &mut dyn TextureSink {
        &mut self.atlas
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        Some(self)
    }
}
