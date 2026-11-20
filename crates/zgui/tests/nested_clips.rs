//! A rounded `overflow: hidden` box inside two rounded clipping ancestors, from a style sheet to
//! the clip the shader reads.

use std::sync::Arc;

use zgui::platform::{Surface, SurfaceEvent};
use zgui::prelude::*;
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{AppError, Runtime};
use zgui::view::{Anchor, BuildCx};
use zgui_platform_headless::Harness;

const SHEET: &str = zgui::css!(
    "root { display: block; width: 300px; height: 200px }
     .r { display: block; overflow: hidden; border-radius: 12px; padding: 16px }
     .clip { display: block; overflow: hidden; border-radius: 24px; width: 200px; height: 120px }
     .fill { display: block; width: 100%; height: 100%; background-color: #ff0000 }"
);

/// The application, with a rounded clip inside two rounded clipping ancestors.
fn mounted() -> Harness<Runtime> {
    let handler = zgui::runtime::App::new()
        .with_title("nested-clips")
        .with_size(300.0, 200.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(
            |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = zgui_testkit_scene::CaptureRenderer::new();
                Renderer::configure(&mut renderer, target);
                Ok::<_, AppError>(Box::new(renderer) as Box<dyn Renderer>)
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(
                view! {
                    box(class = "root") {
                        box(class = "r") { box(class = "r") { box(class = "clip") {
                            box(class = "fill") {}
                        } } }
                    }
                }
                .into_view()
                .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.deliver_to_first(SurfaceEvent::Resized(zgui::geom::Size::new(
        zgui::geom::DevicePx(300.0),
        zgui::geom::DevicePx(200.0),
    )));
    harness.settle(32);
    harness
}

#[test]
fn a_third_rounded_clip_keeps_its_corners() {
    let mut harness = mounted();
    let window = &mut harness.app_mut().windows_mut()[0];
    let scene = window.scene();

    let fill = scene
        .primitives
        .quads
        .iter()
        .find(|quad| quad.clip_id() != zgui_space::ClipId::ROOT && quad.radii[0] == 0.0)
        .expect("the fill is drawn through the clips around it");
    assert!(
        scene.clips.rounded_links(fill.clip_id()) >= 3,
        "the fixture has to nest three rounded clips"
    );
    let resolved = scene.clips.resolve(fill.clip_id());
    let applied = &resolved.rounded[..resolved.rounded_count as usize];
    assert!(
        applied.iter().any(|test| test.radii[0] == 24.0),
        "the innermost clip's corners are applied: {applied:?}"
    );
    assert_eq!(resolved.dropped, 0);
}
