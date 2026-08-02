//! The two contracts, exercised together over a real display list.
//!
//! Nothing in this crate implements either trait, so without this the surface would only be checked
//! for compiling. What is checked here is that the two compose the way the frame sequence needs them
//! to: a scene is finished, its plan is resourced, the scratch is cleared before anything reads it,
//! and the work runs — with no device anywhere.

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::{Css, Device, DevicePx, Point, Rect, Scale, Size};
use zgui_scene::kurbo::{self, Shape};
use zgui_scene::{ClipLink, PaintRef, Quad, Scene, ScenePassPlan, VectorId, VectorItem};

use crate::capabilities::RenderCapabilities;
use crate::memory::MemoryReport;
use crate::outcome::{FrameOutcome, FrameStats, SkipReason};
use crate::renderer::Renderer;
use crate::target::RenderTarget;
use crate::texture::{ExternalTexture, TextureHandle};
use crate::vector::{VectorError, VectorFrame, VectorPass, VectorPlan, VectorRaster, VectorTarget};

/// A device rectangle.
fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// A scene with one quad and two clipped paths beside each other.
fn scene() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(256, 256));

    let fill = {
        let id = scene.paints.solid(Color::srgb(0.5, 0.5, 0.5, 1.0));
        PaintRef::solid(id)
    };
    let card = rect(0.0, 0.0, 256.0, 64.0);
    scene.push_quad(Quad::filled(card, fill));

    let clip = scene.clips.only(ClipLink::rect(card));
    for index in 0..2u32 {
        let bounds = rect(8.0 + index as f32 * 64.0, 8.0, 48.0, 48.0);
        let path = Arc::new(
            kurbo::Rect::new(
                bounds.origin.x.0 as f64,
                bounds.origin.y.0 as f64,
                (bounds.origin.x.0 + bounds.size.width.0) as f64,
                (bounds.origin.y.0 + bounds.size.height.0) as f64,
            )
            .to_path(0.1),
        );
        scene.push_vector(VectorItem::filled(VectorId(index), path, fill).clipped(clip));
    }
    scene.finish(&DamageSet::full());
    scene
}

/// A rasteriser that records what it was asked to do and rasterises nothing.
#[derive(Default)]
struct RecordingRaster {
    /// Which targets were cleared, in order.
    cleared: Vec<VectorTarget>,
    /// Which targets were rasterised, in order.
    rasterised: Vec<VectorTarget>,
}

impl VectorRaster for RecordingRaster {
    fn plan(&mut self, passes: &ScenePassPlan) -> VectorPlan {
        let mut plan = VectorPlan::resourcing(passes);
        for (index, pass) in passes.passes.iter().enumerate() {
            plan.passes.push(VectorPass {
                region: pass.region,
                target: VectorTarget(index as u64),
                items: pass.items.clone(),
                clip: pass.clip,
                instanced: pass.instanced,
            });
        }
        plan
    }

    fn clear_targets(&mut self, plan: &VectorPlan) {
        self.cleared
            .extend(plan.passes.iter().map(|pass| pass.target));
    }

    fn prepare(&mut self, frame: &mut VectorFrame<'_>) -> Result<(), VectorError> {
        for pass in &frame.plan.passes {
            assert!(
                self.cleared.contains(&pass.target),
                "a scratch must be cleared before anything reads it"
            );
            for item in frame.plan.items_of(pass) {
                // Every residual has to be resolvable through the table travelling with the frame.
                let _ = frame.clips.resolve(item.residual);
                let _ = &frame.items[item.item];
            }
            self.rasterised.push(pass.target);
        }
        Ok(())
    }

    fn memory(&self) -> MemoryReport {
        MemoryReport {
            fixed: 1024,
            ..MemoryReport::ZERO
        }
    }
}

/// A renderer that runs the frame sequence and draws nothing.
#[derive(Default)]
struct RecordingRenderer {
    /// The surface, once configured.
    target: Option<RenderTarget>,
    /// The rasteriser it drives.
    raster: RecordingRaster,
    /// External textures it has been handed.
    externals: Vec<ExternalTexture>,
    /// Where its atlas tiles would go.
    atlas: zgui_atlas::MemorySink,
}

impl Renderer for RecordingRenderer {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        if self.target.is_none() {
            return FrameOutcome::Skipped(SkipReason::Unconfigured);
        }
        assert!(scene.is_finished(), "a renderer executes a finished scene");

        let plan = self.raster.plan(scene.pass_plan());
        if !plan.is_empty() {
            self.raster.clear_targets(&plan);
            let placements = zgui_scene::Placements::of(&scene.spatial);
            let mut frame = VectorFrame::new(
                &plan,
                &scene.primitives.vectors,
                &scene.clips,
                &scene.paints,
                &placements,
            );
            self.raster.prepare(&mut frame).expect("nothing can fail");
        }

        FrameOutcome::Presented(FrameStats {
            draw_calls: scene.batches().count() as u32,
            vector_passes: plan.len() as u32,
            damage_px: damage
                .area()
                .unwrap_or_else(|| self.target.map(|target| target.area() as i64).unwrap_or(0))
                as u64,
            bytes_uploaded: 0,
            memory: self.memory(),
        })
    }

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        &mut self.atlas
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        self.externals.push(texture);
        texture.handle
    }

    fn release_external(&mut self, handle: TextureHandle) {
        self.externals.retain(|held| held.handle != handle);
    }

    fn memory(&self) -> MemoryReport {
        MemoryReport {
            targets: 4096,
            ..MemoryReport::ZERO
        }
        .plus(self.raster.memory())
    }
}

#[test]
fn an_unconfigured_renderer_skips_before_recording_anything_and_keeps_its_damage() {
    let mut renderer = RecordingRenderer::default();
    let outcome = renderer.draw(&scene(), &DamageSet::full());

    assert_eq!(outcome, FrameOutcome::Skipped(SkipReason::Unconfigured));
    assert!(!outcome.retires_damage());
    assert!(renderer.raster.rasterised.is_empty());
}

#[test]
fn the_frame_sequence_clears_every_scratch_before_it_is_rasterised() {
    let mut renderer = RecordingRenderer::default();
    renderer.configure(RenderTarget::new(
        Size::new(256, 256),
        Scale::<Css, Device>::new(1.0),
    ));

    let scene = scene();
    let outcome = renderer.draw(&scene, &DamageSet::full());

    let stats = outcome.stats().expect("presented");
    assert_eq!(stats.vector_passes, scene.pass_plan().len() as u32);
    assert_eq!(renderer.raster.cleared, renderer.raster.rasterised);
    assert!(!renderer.raster.rasterised.is_empty());
    assert!(outcome.retires_damage());
}

#[test]
fn a_frame_with_no_vector_work_reaches_the_rasteriser_for_nothing_but_planning() {
    let mut renderer = RecordingRenderer::default();
    renderer.configure(RenderTarget::new(
        Size::new(64, 64),
        Scale::<Css, Device>::new(1.0),
    ));

    let mut scene = Scene::new();
    scene.begin_frame(Size::new(64, 64));
    let fill = {
        let id = scene.paints.solid(Color::srgb(1.0, 0.0, 0.0, 1.0));
        PaintRef::solid(id)
    };
    scene.push_quad(Quad::filled(rect(0.0, 0.0, 16.0, 16.0), fill));
    scene.finish(&DamageSet::full());

    renderer.draw(&scene, &DamageSet::full());
    assert!(renderer.raster.cleared.is_empty());
    assert!(renderer.raster.rasterised.is_empty());
}

#[test]
fn a_renderer_aggregates_its_rasterisers_memory_into_its_own() {
    let renderer = RecordingRenderer::default();
    assert_eq!(renderer.memory().fixed, 1024);
    assert_eq!(renderer.memory().targets, 4096);
    assert_eq!(renderer.memory().total(), 5120);
}

#[test]
fn an_external_texture_can_be_registered_and_forgotten() {
    let mut renderer = RecordingRenderer::default();
    let texture = ExternalTexture {
        id: zgui_scene::ExternalTextureId(7),
        handle: TextureHandle(42),
        size: Size::new(320, 240),
        premultiplied: true,
    };
    assert_eq!(renderer.register_external(texture), TextureHandle(42));
    assert_eq!(renderer.externals.len(), 1);
    renderer.release_external(TextureHandle(42));
    assert!(renderer.externals.is_empty());
}
