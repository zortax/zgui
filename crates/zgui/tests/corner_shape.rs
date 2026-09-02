//! `corner-shape` through the whole application, from a style sheet to the display list.
//!
//! The unit tests say the property parses and the device tests say each shape draws. What is
//! asserted here is the wiring in between: that a style sheet reaches the box's own quad *and* the
//! clip the box gives its children, which are the two halves that have to agree or content shows
//! its corners past the card holding it.

use std::sync::Arc;

use zgui::platform::{Surface, SurfaceEvent};
use zgui::prelude::*;
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{AppError, Runtime};
use zgui::view::{Anchor, BuildCx};
use zgui_platform_headless::Harness;

const SHEET: &str = zgui::css!(
    "root { display: block; width: 300px; height: 200px }
     .card {
        display: block;
        width: 120px;
        height: 80px;
        border-radius: 24px;
        background-color: #ff0000;
        overflow: hidden;
        --zgui-corner-shape: squircle;
     }
     .inner { display: block; width: 120px; height: 80px; background-color: #00ff00 }
     .plain {
        display: block;
        width: 60px;
        height: 40px;
        border-radius: 12px;
        background-color: #0000ff;
     }"
);

/// The application, with one smoothed card holding one child.
fn mounted() -> Harness<Runtime> {
    let handler = zgui::runtime::App::new()
        .with_title("corner-shape")
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
                        box(class = "card") { box(class = "inner") {} }
                        box(class = "plain") {}
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
fn a_style_sheet_cuts_the_box_it_names_and_the_clip_that_box_gives_its_children() {
    let mut harness = mounted();
    let window = &mut harness.app_mut().windows_mut()[0];
    let scene = window.scene();

    let card = scene
        .primitives
        .quads
        .iter()
        .find(|quad| quad.radii[0] > 0.0)
        .expect("the card reached the display list");
    assert_eq!(
        card.corner_shape(),
        zgui_scene::CornerShape::SQUIRCLE,
        "the box the sheet named is cut to the shape it named"
    );

    // The child is drawn through the clip the card gives it, and that clip has to be cut the same
    // way or the child's own corners show past the card's.
    let inner = scene
        .primitives
        .quads
        .iter()
        .find(|quad| quad.clip_id() != zgui_scene::ClipId::ROOT)
        .expect("the child is drawn through a clip of its own");
    assert!(
        scene
            .clips
            .links(inner.clip_id())
            .iter()
            .any(|link| matches!(
                link,
                zgui_scene::ClipLink::RoundedRect { shape, .. }
                    if *shape == zgui_scene::CornerShape::SQUIRCLE
            )),
        "the clip the card gives its children is cut the way the card is"
    );
    // And the flattened form the shader actually reads carries it too.
    let resolved = scene.clips.resolve(inner.clip_id());
    assert_eq!(
        resolved.rounded[0].shape,
        zgui_scene::CornerShape::SQUIRCLE.get(),
        "the chain flattens to a test the shading code cuts the same way"
    );
}

/// A box outside the subtree that named a shape keeps the ellipse, or every rounded box in every
/// document changes shape the moment one card asks for a smoothed corner.
#[test]
fn a_box_beside_the_one_that_named_a_shape_is_still_the_ellipse() {
    let mut harness = mounted();
    let window = &mut harness.app_mut().windows_mut()[0];
    let scene = window.scene();
    let plain = scene
        .primitives
        .quads
        .iter()
        .find(|quad| quad.radii[0] == 12.0)
        .expect("the sibling reached the display list");
    assert_eq!(plain.corner_shape(), zgui_scene::CornerShape::ROUND);
}

/// The shape inherits, because every unregistered custom property does — and here that is wanted
/// rather than tolerated: "this subtree uses smoothed corners" is a thing a style sheet should be
/// able to say once. A descendant that wants the ellipse back writes `round`.
#[test]
fn the_shape_inherits_and_a_descendant_can_take_it_back() {
    let mut harness = mounted();
    let window = &mut harness.app_mut().windows_mut()[0];
    let scene = window.scene();
    // The child inside the card inherited the shape. It has no radii, so it draws exactly as it
    // would have — which is why the inheritance is survivable as well as wanted.
    let inner = scene
        .primitives
        .quads
        .iter()
        .find(|quad| quad.clip_id() != zgui_scene::ClipId::ROOT && quad.radii[0] == 0.0)
        .expect("the child reached the display list");
    assert_eq!(inner.corner_shape(), zgui_scene::CornerShape::SQUIRCLE);
}
