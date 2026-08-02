//! What a real document actually draws, for two declarations that drew nothing at all.
//!
//! Both defects here passed every test in the workspace. They could, because every assertion about
//! opacity was written over a hand-built scene and every assertion about text decoration was
//! written over a hand-built [`DecorationStyle`], and in both cases the thing that was broken sat
//! *between* the style engine and the display list — which is exactly the span no hand-built input
//! crosses. So these mount a document, style it with the declaration in question, and ask what the
//! frame put in the display list.
//!
//! Each case carries the control that stops it passing while the feature is dead. A translucent box
//! is asserted to be *present*, at the alpha it was given, and beside a sibling that composites
//! into a target of its own — because the failure was a primitive being swept into that sibling's
//! target and clipped away, and a case with no such sibling never sees it. A decorated line is
//! asserted to have a decoration primitive, in the colour the sheet named, over the line box the
//! text landed in — because the failure was no primitive at all, and "the style says line-through"
//! is a claim about the cascade rather than about the frame.
//!
//! [`DecorationStyle`]: zgui_paint::emit::text::DecorationStyle

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_platform::Surface;
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle,
};
use zgui_runtime::{App, AppError};
use zgui_scene::Scene;
use zgui_view::{Anchor, BuildCx, IntoView, View};

/// One quad of a frame, reduced to what these cases ask about.
#[derive(Clone, Copy, Debug)]
struct Quad {
    /// Where it falls in the painting order.
    ///
    /// The order is the whole of the opacity case. A primitive that is *in* the display list is
    /// not thereby drawn where it belongs: a group's closing marker is a primitive too, and a quad
    /// that sorts ahead of one is drawn into a target that has already finished and clipped away
    /// by that target's bounds. An assertion over the array alone cannot see that, and passes.
    order: u32,
    /// Its solid fill, when it has one.
    fill: Option<Color>,
    /// Its rectangle, as `[x, y, width, height]`.
    bounds: [f32; 4],
}

/// One group marker of a frame.
#[derive(Clone, Copy, Debug)]
struct Marker {
    /// Where it falls in the painting order.
    order: u32,
    /// Whether it opens the group rather than closing it.
    is_start: bool,
}

/// One decoration line of a frame.
#[derive(Clone, Copy, Debug)]
struct Line {
    /// Its colour, premultiplied and gamma-encoded, as the display list carries it.
    color: [f32; 4],
    /// Its rectangle, as `[x, y, width, height]`.
    bounds: [f32; 4],
    /// Which of the five shapes it is.
    style: u32,
}

/// What one frame put in the display list.
#[derive(Clone, Debug, Default)]
struct Frame {
    /// Its quads.
    quads: Vec<Quad>,
    /// Its decoration lines.
    lines: Vec<Line>,
    /// The group markers it pushed.
    markers: Vec<Marker>,
}

/// The frames a run produced, in order.
type Log = Rc<RefCell<Vec<Frame>>>;

/// A renderer that records the display list and draws nothing.
struct Recorder {
    /// Where the frames go.
    log: Log,
    /// The surface it was pointed at.
    target: Option<RenderTarget>,
    /// Tiles go into plain memory, so the upload path still runs.
    atlas: zgui_atlas::MemorySink,
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

    fn draw(&mut self, scene: &Scene, _damage: &DamageSet) -> FrameOutcome {
        self.log.borrow_mut().push(Frame {
            quads: scene
                .primitives
                .quads
                .iter()
                .map(|quad| Quad {
                    order: quad.order,
                    fill: quad.fill.id().and_then(|id| match scene.paints.get(id) {
                        Some(zgui_scene::Paint::Solid(color)) => Some(*color),
                        _ => None,
                    }),
                    bounds: quad.bounds,
                })
                .collect(),
            lines: scene
                .primitives
                .decorations
                .iter()
                .map(|line| Line {
                    color: line.color,
                    bounds: line.bounds,
                    style: line.style,
                })
                .collect(),
            markers: scene
                .primitives
                .groups
                .iter()
                .map(|group| Marker {
                    order: group.order,
                    is_start: group.is_start,
                })
                .collect(),
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

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        &mut self.atlas
    }
}

/// Mounts a document styled by `css` and returns the frames it drew.
fn frames(
    css: &'static str,
    view: impl FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
) -> Vec<Frame> {
    let log: Log = Rc::default();
    let factory = Rc::clone(&log);
    let handler = App::new()
        .with_title("appearance")
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Recorder {
                    log: Rc::clone(&factory),
                    target: None,
                    atlas: zgui_atlas::MemorySink::default(),
                };
                renderer.configure(target);
                Ok::<Box<dyn Renderer>, AppError>(Box::new(renderer))
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .with_glyph_raster(Box::new(|| Arc::new(zgui_testkit_scene::MonoRaster::new())))
        .into_handler(view)
        .expect("the reactive runtime installs");
    let mut harness = zgui_platform_headless::Harness::new(handler);
    harness.settle(4);
    let drawn = log.borrow().clone();
    assert!(
        !drawn.is_empty(),
        "mounting a document drew no frame at all"
    );
    drawn
}

/// The sheet the opacity cases are styled by.
///
/// The filtered sibling is not decoration. A filter forces a target of its own, and the defect was
/// that the marker closing that target and the next thing drawn were free to take the same place in
/// the order — so the translucent box was drawn *into* a target that had already finished, and
/// clipped away by its bounds. Without the sibling there is no target and nothing to be swept into.
const OPACITY_CSS: &str = "root { display: block; width: 400px; height: 300px }
     .row { display: flex; gap: 20px }
     .swatch { width: 40px; height: 40px; background-color: #ff0000 }
     .filtered { filter: saturate(0.25) }
     .faded { opacity: 0.45 }";

/// The sheet the decoration cases are styled by.
const DECORATION_CSS: &str = "root { display: block; width: 400px; height: 300px }
     .struck { display: block; text-decoration: line-through; text-decoration-color: #ff0000 }
     .wavy { display: block; text-decoration: underline wavy #00ff00 }
     .quiet { display: block }
     .over { display: block; text-decoration: overline; text-decoration-color: #0000ff }";

/// A row of a filtered swatch and a translucent one.
fn opacity_row(cx: &mut BuildCx<'_>) -> Box<dyn Anchor> {
    Box::new(
        zgui_elements::column()
            .class("root")
            .child(
                zgui_elements::row()
                    .class("row")
                    .child(zgui_elements::column().class("swatch filtered"))
                    .child(zgui_elements::column().class("swatch faded")),
            )
            .into_view()
            .build(cx),
    )
}

#[test]
fn a_translucent_box_after_a_filtered_sibling_is_still_drawn() {
    let drawn = frames(OPACITY_CSS, opacity_row);
    let first = &drawn[0];

    let opened = first
        .markers
        .iter()
        .find(|marker| marker.is_start)
        .copied()
        .expect("the filtered sibling opened no target, so this case cannot see its own defect");
    let closed = first
        .markers
        .iter()
        .find(|marker| !marker.is_start)
        .copied()
        .expect("a group that is opened is closed");

    let opaque = first
        .quads
        .iter()
        .find(|quad| quad.fill.is_some_and(|fill| fill.alpha() >= 1.0))
        .copied()
        .expect("the filtered swatch is missing, so there is nothing to compare against");
    assert!(
        opaque.order > opened.order && opaque.order < closed.order,
        "the control is wrong: the filtered swatch must be drawn *inside* its own target, or the \
         assertion below is about nothing — swatch {opaque:?} in {opened:?}..{closed:?}"
    );

    let faded = first
        .quads
        .iter()
        .find(|quad| {
            quad.fill
                .is_some_and(|fill| (fill.alpha() - 0.45).abs() < 1e-3)
        })
        .copied()
        .expect("`opacity: 0.45` put no quad in the display list at all");
    assert_eq!(
        faded.bounds[2..],
        [40.0, 40.0],
        "the translucent box is drawn at its own size"
    );
    assert!(
        faded.bounds[0] > opaque.bounds[0],
        "it is drawn where the layout put it, beside the sibling rather than under it"
    );
    assert!(
        faded.order > closed.order,
        "the translucent box sorts inside a group that has already closed, so it is drawn into \
         that group's target and clipped away by its bounds: box {faded:?}, the group closing at \
         {closed:?}"
    );
}

#[test]
fn every_line_of_text_inside_a_decorated_box_is_decorated() {
    // The declaration is on the element; the line box belongs to the anonymous inline root
    // generated under it, whose own style carries no decoration because decoration is not an
    // inherited property. So a decoration read off the line's own box is always absent.
    let drawn = frames(DECORATION_CSS, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::text().class("struck").child(|| "abc"))
                .child(zgui_elements::text().class("quiet").child(|| "def"))
                .into_view()
                .build(cx),
        )
    });
    let first = &drawn[0];

    assert_eq!(
        first.lines.len(),
        1,
        "one decorated element and one undecorated one draw exactly one line: {:?}",
        first.lines
    );
    let line = first.lines[0];
    assert_eq!(
        line.color,
        [1.0, 0.0, 0.0, 1.0],
        "the line is drawn in `text-decoration-color`, not in the text's own colour"
    );
    assert!(
        line.bounds[2] > 0.0 && line.bounds[3] > 0.0,
        "a line with no extent is a primitive that draws nothing: {line:?}"
    );
    // A strike sits inside the line box it crosses rather than under it, which is what says the
    // line was placed against the text rather than at some fixed offset from the box.
    assert!(
        line.bounds[1] > 0.0 && line.bounds[1] < 24.0,
        "the strike is not inside the line box it crosses: {line:?}"
    );
}

#[test]
fn a_decoration_reaches_the_text_of_a_descendant_that_declared_nothing() {
    // CSS propagates a decoration from the box that declared it to its in-flow descendants, which
    // is why it is not an inherited property: the descendants draw the *ancestor's* line, in the
    // ancestor's colour. A framework that reads the property off the line's own box draws nothing
    // here whatever it does in the direct case.
    let drawn = frames(DECORATION_CSS, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::column()
                        .class("over")
                        .child(zgui_elements::text().class("quiet").child(|| "abc")),
                )
                .into_view()
                .build(cx),
        )
    });
    let first = &drawn[0];
    assert_eq!(first.lines.len(), 1, "{:?}", first.lines);
    assert_eq!(
        first.lines[0].color,
        [0.0, 0.0, 1.0, 1.0],
        "the descendant drew the ancestor's colour, which is what propagation means"
    );
}

#[test]
fn a_wave_is_given_a_band_it_can_have_a_shape_in() {
    // The shader evaluates a wave across the whole rectangle it is handed: the amplitude is what is
    // left of the height once the stroke is drawn. Handed a rectangle one stroke tall — which is
    // what a solid line wants — a wave is a straight line, drawn and indistinguishable from the
    // decoration it is not.
    let drawn = frames(DECORATION_CSS, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::text().class("wavy").child(|| "abc"))
                .child(zgui_elements::text().class("struck").child(|| "def"))
                .into_view()
                .build(cx),
        )
    });
    let first = &drawn[0];
    assert_eq!(first.lines.len(), 2, "{:?}", first.lines);

    let wavy = first
        .lines
        .iter()
        .find(|line| line.style == zgui_scene::prim::decoration::DecorationStyle::Wavy as u32)
        .expect("`text-decoration: underline wavy` produced no wavy line");
    let solid = first
        .lines
        .iter()
        .find(|line| line.style == zgui_scene::prim::decoration::DecorationStyle::Solid as u32)
        .expect("the solid control is missing, so the comparison below proves nothing");

    assert!(
        wavy.bounds[3] > solid.bounds[3],
        "a wave needs more height than a stroke or it has no amplitude: wavy {wavy:?} solid \
         {solid:?}"
    );
    assert!(
        wavy.bounds[3] >= solid.bounds[3] * 3.0,
        "the band is three strokes tall, which is what the shader's wave and its doubled line are \
         written against: {wavy:?}"
    );
}
