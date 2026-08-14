//! What a real window draws for an element carrying outlines.
//!
//! The paint stage's own tests drive the emit walk directly, which proves the walk. They cannot
//! prove the window: the outlines are read through a source the *frame loop* installs, and a frame
//! that never installed one would emit nothing while every paint test stayed green. So this mounts
//! a document through the real runtime, runs real frames against the headless platform, and asks
//! the renderer what it was handed.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_elements::kurbo::BezPath;
use zgui_platform::Surface;
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle, VectorBackend, VectorStatus,
};
use zgui_runtime::{App, AppError};
use zgui_scene::Scene;
use zgui_view::{Anchor, BuildCx, IntoView, View};

/// One vector item of a frame, reduced to what these cases ask about.
#[derive(Clone, Debug)]
struct Drawn {
    /// What strokes it and how wide, when anything does.
    stroke: Option<(Color, f32)>,
}

/// One eligible vector shape emitted through the monochrome atlas fast path.
#[derive(Clone, Copy, Debug)]
struct Masked {
    /// Where the mask lands, as x, y, width and height.
    bounds: [f32; 4],
    /// The tint inherited from the element.
    color: [f32; 4],
    /// The atlas identity, which changes when the rasterised geometry changes.
    tile: zgui_scene::SpriteTile,
}

/// One frame: what it drew, and what it was allowed to draw over.
///
/// The damage has to be read here rather than off the window afterwards, because a window retires
/// its damage the moment the frame is submitted — a test asking later sees an empty set and passes
/// whatever the frame did.
#[derive(Clone, Debug)]
struct Frame {
    /// The vector content of the display list.
    shapes: Vec<Drawn>,
    /// Eligible vector content emitted as CPU-rasterised atlas masks.
    masks: Vec<Masked>,
    /// Whether the frame was drawn against the whole surface.
    full: bool,
    /// The rectangle the damage rectangles are bounded by, when the set is not full and not empty.
    damaged: Option<zgui_geom::Rect<i32, zgui_geom::Device>>,
}

/// The frames a run produced.
type Log = Rc<RefCell<Vec<Frame>>>;

/// A renderer that records the vector content of every frame and draws nothing.
struct Recorder {
    /// Where the frames go.
    log: Log,
    /// The surface it was pointed at.
    target: Option<RenderTarget>,
    /// Tiles go into plain memory, so the upload path still runs.
    atlas: zgui_atlas::MemorySink,
    /// Whether a frame containing a general vector pass has paid the fake backend's lazy cost.
    initialized: bool,
}

impl Renderer for Recorder {
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
        if !scene.pass_plan().is_empty() {
            self.initialized = true;
        }
        let solid = |reference: zgui_scene::PaintRef| {
            reference.id().and_then(|id| match scene.paints.get(id) {
                Some(zgui_scene::Paint::Solid(color)) => Some(*color),
                _ => None,
            })
        };
        self.log.borrow_mut().push(Frame {
            shapes: scene
                .primitives
                .vectors
                .iter()
                .map(|item| Drawn {
                    stroke: item.stroke.as_ref().and_then(|stroke| {
                        solid(stroke.paint).map(|color| (color, stroke.width()))
                    }),
                })
                .collect(),
            masks: scene
                .primitives
                .mono_sprites
                .iter()
                .map(|sprite| Masked {
                    bounds: sprite.bounds,
                    color: sprite.color,
                    tile: sprite.tile,
                })
                .collect(),
            full: damage.is_full(),
            damaged: damage.bounds(),
        });
        FrameOutcome::Presented(zgui_render::FrameStats {
            vector_passes: 0,
            draw_calls: 0,
            damage_px: 0,
            bytes_uploaded: 0,
            memory: MemoryReport::ZERO,
        })
    }

    fn register_external(&mut self, _texture: ExternalTexture) -> TextureHandle {
        TextureHandle(0)
    }

    fn release_external(&mut self, _handle: TextureHandle) {}

    fn memory(&self) -> MemoryReport {
        MemoryReport::ZERO
    }

    fn vector_status(&self) -> VectorStatus {
        VectorStatus {
            backend: Some(VectorBackend::Vello),
            initialized: self.initialized,
        }
    }

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        &mut self.atlas
    }
}

/// Mounts a document styled by `css` and returns the window, with every frame recorded into `log`.
fn mount(
    css: &'static str,
    log: &Log,
    view: impl FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    let factory = Rc::clone(log);
    let handler = App::new()
        .with_title("drawing")
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Recorder {
                    log: Rc::clone(&factory),
                    target: None,
                    atlas: zgui_atlas::MemorySink::default(),
                    initialized: false,
                };
                renderer.configure(target);
                Ok::<Box<dyn Renderer>, AppError>(Box::new(renderer))
            },
        ))
        .into_handler(view)
        .expect("the reactive runtime installs");
    zgui_platform_headless::Harness::new(handler)
}

/// Runtime diagnostics keep the emitter's real choice, aggregate it through an icon wrapper, and
/// retain the element keys from the exact frame that constructed the lazy backend.
#[test]
fn vector_diagnostics_name_the_route_wrapper_and_vello_initializer() {
    let log: Log = Rc::default();
    let mut app = mount(CSS, &log, |cx| {
        Box::new(
            zgui_elements::r#box()
                .class("wrapper")
                .child(
                    zgui_elements::vector()
                        .class("icon")
                        .class("thick")
                        .view_box(0.0, 0.0, 24.0, 24.0)
                        .paths([triangle()]),
                )
                .into_view()
                .build(cx),
        )
    });
    app.settle(4);
    let window = &app.app().windows()[0];
    let icon = {
        let layout = window.layout().borrow();
        layout
            .keys()
            .into_iter()
            .flat_map(|box_| layout.fragments_of_box(box_).iter().copied())
            .filter_map(|fragment| layout.fragment(fragment))
            .find(|fragment| fragment.kind == zgui_layout::FragmentKind::Vector)
            .and_then(|fragment| fragment.node)
            .expect("the icon has a vector fragment")
    };
    let wrapper = {
        let document = window.document().borrow();
        let icon = document.store().index_of(icon).expect("the icon is live");
        let parent = document
            .store()
            .core(icon)
            .parent()
            .expect("the icon is wrapped");
        document.store().key_of(parent)
    };

    assert!(
        window
            .vector_routes(icon)
            .contains(zgui_paint::VectorRoute::GeneralRaster)
    );
    assert!(window.vector_routes(wrapper).is_empty());
    assert!(
        window
            .vector_routes_in_subtree(wrapper)
            .contains(zgui_paint::VectorRoute::GeneralRaster)
    );
    assert_eq!(window.vello_initializers(), &[icon]);
}

/// Mounts a document styled by `css` and returns the frames it drew.
fn frames(
    css: &'static str,
    view: impl FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
) -> Vec<Frame> {
    let log: Log = Rc::default();
    let mut harness = mount(css, &log, view);
    harness.settle(4);
    let drawn = log.borrow().clone();
    assert!(!drawn.is_empty(), "mounting a document drew no frame");
    drawn
}

/// The sheet the cases below are styled by. The icon is themed the way a component library themes
/// one: by the element's own `color`, with no property naming the drawing at all.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
     .icon { display: block; width: 32px; height: 32px; color: rgb(255, 0, 0) }
     /* Turned off the axes, which is what keeps this shape on the general rasteriser. A fill and a
        stroke together are two tinted coverage tiles when the shape stands square to the device,
        and the two cases below are about the vector route: what a stroke paint carries into a
        vector item, and which element the diagnostics blame for building the rasteriser. */
     .thick { --zgui-stroke: rgb(0, 0, 255); --zgui-stroke-width: 2px;
              transform: rotate(30deg) }";

/// The outline every case draws: a triangle filling a twenty-four unit square.
fn triangle() -> BezPath {
    BezPath::from_svg("M0 0 L24 0 L24 24 Z").expect("the fixture parses")
}

/// The last frame's shapes, which is the settled picture.
///
/// The *last* frame rather than the last one that drew anything: a picture that was right for a
/// frame and empty afterwards is a blank window, and a helper that skipped back to the good frame
/// would report it as a drawing.
fn settled(frames: Vec<Frame>) -> Vec<Drawn> {
    frames
        .into_iter()
        .next_back()
        .map(|frame| frame.shapes)
        .unwrap_or_default()
}

/// The settled frame's eligible vector masks.
fn settled_masks(frames: Vec<Frame>) -> Vec<Masked> {
    frames
        .into_iter()
        .next_back()
        .map(|frame| frame.masks)
        .unwrap_or_default()
}

/// The defect this whole change is about, asserted where an application would meet it.
#[test]
fn a_vector_element_mounted_in_a_window_reaches_the_renderer() {
    let masks = settled_masks(frames(CSS, |cx| {
        Box::new(
            zgui_elements::r#box()
                .child(
                    zgui_elements::vector()
                        .class("icon")
                        .view_box(0.0, 0.0, 24.0, 24.0)
                        .paths([triangle()]),
                )
                .into_view()
                .build(cx),
        )
    }));

    assert_eq!(masks.len(), 1, "one outline is one mask: {masks:?}");
    assert_eq!(
        masks[0].color,
        [1.0, 0.0, 0.0, 1.0],
        "an icon takes the colour of the text around it, with nothing else said"
    );
    assert_eq!(
        (masks[0].bounds[2], masks[0].bounds[3]),
        (32.0, 32.0),
        "the outline is drawn at the size CSS gave the element, not at the size it was written"
    );
}

/// A stroke declared through the custom-property scheme reaches the window's display list, which
/// is the only route there is: this build has no `stroke` property for a sheet to set.
#[test]
fn a_stroke_declared_as_a_custom_property_reaches_the_window() {
    let shapes = settled(frames(CSS, |cx| {
        Box::new(
            zgui_elements::r#box()
                .child(
                    zgui_elements::vector()
                        .class("icon")
                        .class("thick")
                        .view_box(0.0, 0.0, 24.0, 24.0)
                        .paths([triangle()]),
                )
                .into_view()
                .build(cx),
        )
    }));

    let stroked = shapes
        .iter()
        .find_map(|shape| shape.stroke)
        .expect("the drawing was stroked");
    assert_eq!(
        stroked.0.to_premultiplied_srgb(),
        [0.0, 0.0, 1.0, 1.0],
        "{shapes:?}"
    );
    assert_eq!(stroked.1, 2.0);
}

/// The extent of the only drawing fragment in a window, in device pixels.
///
/// Read out of the window's own fragment tree rather than written down here, because it is what a
/// damage rectangle is about to be compared against and a constant would compare the test with its
/// own arithmetic.
fn icon_box(window: &zgui_runtime::Window) -> (i32, i32) {
    let layout = window.layout().borrow();
    for key in layout.keys() {
        for frag in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*frag) else {
                continue;
            };
            if fragment.kind == zgui_layout::FragmentKind::Vector {
                return (
                    fragment.border_box.size.width.0.ceil() as i32,
                    fragment.border_box.size.height.0.ceil() as i32,
                );
            }
        }
    }
    panic!("the window holds no drawing fragment");
}

/// An icon swapped for another of the same size, which is the frame a real application draws every
/// time a disclosure arrow turns or a play button becomes a pause button.
///
/// Nothing about that frame moves. The box is the same box, the fragment is the same piece of it,
/// the computed style is identical and the geometry compares equal — so every stage after the
/// property write is free to conclude there is nothing to do. Four separate things have to hold for
/// the new outline to reach the screen, and each of them fails silently on its own: the write has to
/// owe a repaint, the fragment pass has to turn that into the element's own pixels, the replay
/// record has to notice that a fragment which did not move is nonetheless drawing something else,
/// and the placement cache has to hand back new curves rather than the ones it holds. A window is
/// the only place all four are on at once.
#[test]
fn swapping_the_outlines_redraws_the_icon_and_damages_no_more_than_it() {
    /// The second icon: the same twenty-four unit square, filled rather than halved.
    const SQUARE: &str = "M0 0 L24 0 L24 24 L0 24 Z";
    let swapped = zgui_reactive::RwSignal::new(false);
    let log: Log = Rc::default();
    let mut app = mount(CSS, &log, move |cx| {
        Box::new(
            zgui_elements::r#box()
                .child(
                    zgui_elements::vector()
                        .class("icon")
                        .view_box(0.0, 0.0, 24.0, 24.0)
                        .property(
                            zgui_view::PropKey::new(zgui_elements::vector::PATHS),
                            move || {
                                let path = if zgui_reactive::prelude::Get::get(&swapped) {
                                    SQUARE.to_owned()
                                } else {
                                    triangle().to_svg()
                                };
                                zgui_view::PropValue::from(path.as_str())
                            },
                        ),
                )
                .into_view()
                .build(cx),
        )
    });
    app.settle(8);

    let before = settled_masks(log.borrow().clone());
    assert_eq!(before.len(), 1, "{before:?}");
    let triangle_tile = before[0].tile;

    log.borrow_mut().clear();
    zgui_reactive::prelude::Set::set(&swapped, true);
    app.settle(8);

    let after = log.borrow().clone();
    assert!(!after.is_empty(), "swapping the icon drew no frame at all");
    let drawn: Vec<&Masked> = after.iter().flat_map(|frame| &frame.masks).collect();
    assert_eq!(
        drawn.len(),
        1,
        "the swap put the new outline in no display list: {after:?}"
    );
    assert_ne!(
        drawn[0].tile, triangle_tile,
        "the frame after the swap reused the triangle's coverage tile"
    );

    // And it did so without repainting the window. Without the lower bound above this would be
    // satisfied by a frame loop that drew nothing; without this, by one that repaints everything on
    // every write, which is the state incremental damage exists to rule out.
    let icon = icon_box(&app.app().windows()[0]);
    for frame in &after {
        assert!(
            !frame.full,
            "an icon swap damaged the whole window: {frame:?}"
        );
        let Some(bounds) = frame.damaged else {
            continue;
        };
        assert!(
            bounds.size.width <= icon.0 + 2 && bounds.size.height <= icon.1 + 2,
            "an icon swap damaged {bounds:?}, well beyond the {icon:?} pixel icon that changed"
        );
    }
}

/// Two elements drawing the same outline at different sizes: the proof that fitting happens per
/// box rather than once for the notation, which a cache keyed on the notation alone would break.
#[test]
fn two_elements_sharing_one_outline_are_each_fitted_to_their_own_box() {
    let css = "root { display: block; width: 400px; height: 300px }
               .small { display: block; width: 16px; height: 16px }
               .large { display: block; width: 64px; height: 64px }";
    let masks = settled_masks(frames(css, |cx| {
        Box::new(
            zgui_elements::r#box()
                .child(
                    zgui_elements::vector()
                        .class("small")
                        .view_box(0.0, 0.0, 24.0, 24.0)
                        .paths([triangle()]),
                )
                .child(
                    zgui_elements::vector()
                        .class("large")
                        .view_box(0.0, 0.0, 24.0, 24.0)
                        .paths([triangle()]),
                )
                .into_view()
                .build(cx),
        )
    }));

    let mut widths: Vec<f32> = masks.iter().map(|mask| mask.bounds[2]).collect();
    widths.sort_by(f32::total_cmp);
    assert_eq!(widths, vec![16.0, 64.0], "{masks:?}");
}
