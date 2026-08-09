//! A custom element drawing its own glyphs, through the real frame loop.
//!
//! What is asserted is the two things the seam promises. A run the element shaped itself reaches
//! the display list as sprites, at the position the element asked for. And a frame that changes
//! nothing replays those sprites rather than painting again — which only works if every tile they
//! read is still held, so the same test covers the resource half.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use zgui_custom::PaintSlot;
use zgui_custom::{
    CustomElement, CustomLayoutCx, CustomMeasured, FaceId, ScenePainter, ShapedGlyph, ShapedRun,
};
use zgui_geom::{Device, DevicePx, Point};
use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_runtime::{App, AppError, Runtime};
use zgui_view::{Anchor, BuildCx, IntoView, View};

/// How many times the element has painted, so a replayed frame can be told from a painted one.
static PAINTS: AtomicUsize = AtomicUsize::new(0);

/// Where the element puts its line box, in painter coordinates.
fn origin() -> Point<DevicePx, Device> {
    Point::new(DevicePx(4.0), DevicePx(2.0))
}

/// Three glyphs, spaced by the testkit shaper's advance.
const GLYPHS: [ShapedGlyph; 3] = [
    ShapedGlyph {
        glyph: 1,
        x: 0.0,
        y: 12.0,
    },
    ShapedGlyph {
        glyph: 2,
        x: 8.0,
        y: 12.0,
    },
    ShapedGlyph {
        glyph: 3,
        x: 16.0,
        y: 12.0,
    },
];

/// An application whose window records rather than draws, with a glyph rasteriser installed.
fn app<V>(css: &str, view: V) -> Harness<Runtime>
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    let handler = App::new()
        .with_title("glyphs")
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
        .with_renderer(Box::new(
            |_surface: &Arc<dyn Surface>, target| -> Result<_, AppError> {
                let mut renderer = zgui_testkit_scene::CaptureRenderer::new();
                zgui_render::Renderer::configure(&mut renderer, target);
                Ok(Box::new(renderer) as Box<dyn zgui_render::Renderer>)
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .with_glyph_raster(Box::new(|| Arc::new(zgui_testkit_scene::MonoRaster::new())))
        .with_custom(Box::new(|_document| zgui_custom::sources()))
        .into_handler(view)
        .expect("the reactive runtime installs");
    Harness::new(handler)
}

/// A label: fixed size, drawing one run of glyphs it shaped itself.
struct Label;

impl CustomElement for Label {
    fn layout(&mut self, cx: &mut CustomLayoutCx<'_>) -> CustomMeasured {
        CustomMeasured {
            width: cx.known_width.unwrap_or(80.0),
            height: 24.0,
            ..CustomMeasured::default()
        }
    }

    fn paint(&mut self, painter: &mut ScenePainter<'_>) {
        PAINTS.fetch_add(1, Ordering::SeqCst);
        let run = ShapedRun {
            face: FaceId(0),
            size: 16.0,
            synthetic_bold: 0.0,
            synthetic_slant: 0.0,
            has_color: false,
            brush: PaintSlot(0),
            glyphs: &GLYPHS,
        };
        painter.glyphs(&run, origin(), painter.current_color());
    }
}

/// The scene the window last composed.
fn sprites(app: &mut Harness<Runtime>) -> Vec<zgui_paint::MonoSprite> {
    app.app_mut().windows_mut()[0]
        .scene()
        .primitives
        .mono_sprites
        .clone()
}

/// The custom element's border box.
fn custom_box(window: &zgui_runtime::Window) -> (f32, f32) {
    let document = window.document().borrow();
    let layout = window.layout().borrow();
    for key in layout.keys() {
        let node = layout.node(key);
        let Some(source) = node.source else { continue };
        let Some(index) = document.store().index_of(source) else {
            continue;
        };
        if document.store().core(index).local_name().as_str() != "custom" {
            continue;
        }
        let fragment = layout.fragments_of_box(key).first().expect("a fragment");
        let fragment = layout.fragment(*fragment).expect("a fragment");
        return (
            fragment.content_box.origin.x.0,
            fragment.content_box.origin.y.0,
        );
    }
    panic!("the custom element laid out");
}

/// The fixture application, with one label in it.
fn label_app() -> Harness<Runtime> {
    let (view, _handle) = zgui_custom::custom(Label);
    // The handle is dropped: the element stays mounted through the document, and nothing in this
    // test asks it to repaint.
    std::mem::forget(_handle);
    app("root { display: block; width: 400px; height: 300px }", {
        let mut view = Some(view);
        move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::r#box()
                    .class("root")
                    .child(view.take().expect("built once"))
                    .into_view()
                    .build(cx),
            )
        }
    })
}

#[test]
fn an_elements_own_run_reaches_the_display_list_as_sprites() {
    PAINTS.store(0, Ordering::SeqCst);
    let mut app = label_app();
    app.settle(16);

    let drawn = sprites(&mut app);
    assert_eq!(drawn.len(), GLYPHS.len(), "one sprite per glyph");

    let (box_x, box_y) = custom_box(&app.app_mut().windows_mut()[0]);
    // The first glyph's tile starts at the run's origin plus the rasteriser's own bearing, which
    // is what makes this an assertion about placement rather than about the fixture's bitmap.
    let first = drawn
        .iter()
        .map(|sprite| sprite.ink().origin.x.0)
        .fold(f32::MAX, f32::min);
    assert!(
        (first - (box_x + origin().x.0)).abs() <= 2.0,
        "the run landed at the element's content box plus the origin it asked for: \
         {first} against {}",
        box_x + origin().x.0
    );

    let baselines: Vec<f32> = drawn.iter().map(|sprite| sprite.ink().origin.y.0).collect();
    assert!(
        baselines.windows(2).all(|pair| pair[0] == pair[1]),
        "every glyph of one run sits on one baseline"
    );
    assert!(
        baselines[0] > box_y,
        "and below the top of the element's content box"
    );

    // The advances the run asked for survived, in whole pixels.
    let mut xs: Vec<f32> = drawn.iter().map(|s| s.ink().origin.x.0).collect();
    xs.sort_by(f32::total_cmp);
    assert_eq!(xs[1] - xs[0], 8.0);
    assert_eq!(xs[2] - xs[1], 8.0);
}

#[test]
fn an_idle_frame_does_not_ask_the_element_to_paint_again() {
    PAINTS.store(0, Ordering::SeqCst);
    let mut app = label_app();
    app.settle(16);
    let painted = PAINTS.load(Ordering::SeqCst);
    assert!(painted >= 1, "the element painted at least once");

    // Nothing changed, so nothing is damaged and the element's revision has not moved.
    app.settle(16);
    assert_eq!(
        PAINTS.load(Ordering::SeqCst),
        painted,
        "an element whose revision did not move is not asked to paint again"
    );
}

#[test]
fn repainting_the_whole_surface_draws_the_same_glyphs_in_the_same_places() {
    PAINTS.store(0, Ordering::SeqCst);
    let mut app = label_app();
    app.settle(16);
    let first = sprites(&mut app);
    assert_eq!(first.len(), GLYPHS.len());

    // Coming back from occlusion asks for every pixel again while changing no input to any of
    // them. Whether the fragment replays its recorded range or encodes it afresh, the tiles it
    // draws from have to still be there — and with `verify_replays` on under debug assertions, a
    // range whose resources had been released panics here rather than drawing wrongly.
    let surface = app
        .platform()
        .offscreens()
        .first()
        .map(|surface| zgui_platform::Surface::id(surface.as_ref()))
        .expect("the application opened its window");
    app.deliver(surface, zgui_platform::SurfaceEvent::Occluded(true));
    app.settle(8);
    app.deliver(surface, zgui_platform::SurfaceEvent::Occluded(false));
    app.settle(8);
    app.advance(std::time::Duration::from_millis(50));
    app.settle(8);

    let second = sprites(&mut app);
    assert_eq!(
        second.len(),
        first.len(),
        "the repaint drew the element's glyphs again"
    );
    for (before, after) in first.iter().zip(&second) {
        assert_eq!(before.ink(), after.ink(), "at the same place");
        assert_eq!(before.color, after.color, "in the same colour");
        assert_eq!(before.tile, after.tile, "out of the same tile");
    }
}
