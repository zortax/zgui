//! The declaration that makes a box's background paint the text inside it.
//!
//! `background-clip` is a property this engine build discards, so `background-clip: text` cannot be
//! read from a cascade result. The custom-property scheme is the same one drawings are themed
//! through, and this is what says it survives the cascade rather than being a value the paint stage
//! could only be handed directly.

mod support;

use support::{Element, Harness};

/// The lowered paint style of a named element.
fn style_of(harness: &Harness, name: &str) -> zgui_paint::lower::PaintStyle {
    let key = harness.box_of(name);
    zgui_paint::lower::lower(&harness.store().node(key).style, 1.0)
}

/// A root with one child.
fn tree() -> Element {
    Element::new("root").children(vec![Element::new("mark")])
}

#[test]
fn the_declaration_moves_the_ramp_from_the_box_onto_its_text() {
    let css = "root { display: block; width: 200px; height: 100px }
               mark { display: block; width: 120px; height: 40px;
                      background-image: linear-gradient(90deg, rgb(255, 0, 0), rgb(0, 0, 255));
                      --zgui-text-fill: background }";
    let harness = Harness::new(tree(), css);
    let style = style_of(&harness, "mark");

    let ramp = style.text_fill.expect("the ramp paints the text");
    assert_eq!(ramp.stops.len(), 2, "the ramp is the one that was written");
    assert!(
        style.background.layers.is_empty(),
        "a ramp painting the text is a ramp the box no longer paints"
    );
}

#[test]
fn a_box_that_does_not_ask_paints_its_own_background() {
    let css = "root { display: block; width: 200px; height: 100px }
               mark { display: block; width: 120px; height: 40px;
                      background-image: linear-gradient(90deg, rgb(255, 0, 0), rgb(0, 0, 255)) }";
    let harness = Harness::new(tree(), css);
    let style = style_of(&harness, "mark");

    assert!(style.text_fill.is_none(), "nothing asked for it");
    assert_eq!(
        style.background.layers.len(),
        1,
        "and the box keeps its own"
    );
}

/// The property inherits, which is what themes a whole heading and its inline spans at once — and
/// a box with no ramp of its own to hand over paints its text in `color` as it always did.
#[test]
fn asking_without_a_ramp_changes_nothing() {
    let css = "root { display: block; width: 200px; height: 100px;
                      --zgui-text-fill: background }
               mark { display: block; width: 120px; height: 40px }";
    let harness = Harness::new(tree(), css);
    let style = style_of(&harness, "mark");

    assert!(
        style.text_fill.is_none(),
        "there is no ramp to paint the text with, so the text is painted in `color`"
    );
}

/// A ramp painting the text fills the glyphs of the line with one gradient across the line box.
#[test]
fn a_ramp_painting_the_text_fills_its_glyphs() {
    use zgui_css::StyleDraft;
    use zgui_geom::{DevicePx, Point, Rect, Size};
    use zgui_layout::fragment::ParagraphId;
    use zgui_paint::emit::text::{self, GlyphSource, TextPlacement};
    use zgui_paint::lower::background::{GradientShape, GradientSpec, SpecStop};
    use zgui_scene::{ClipId, Scene, SpatialId};

    /// Two glyphs of one run.
    struct TwoGlyphs;

    const GLYPHS: [zgui_text::ShapedGlyph; 2] = [
        zgui_text::ShapedGlyph {
            glyph: 5,
            x: 0.0,
            y: 16.0,
        },
        zgui_text::ShapedGlyph {
            glyph: 6,
            x: 10.0,
            y: 16.0,
        },
    ];

    impl GlyphSource for TwoGlyphs {
        fn visit_line(
            &self,
            _paragraph: ParagraphId,
            _line: u16,
            visit: &mut dyn FnMut(zgui_text::ShapedRun<'_>),
        ) {
            visit(zgui_text::ShapedRun {
                face: zgui_text::FaceId(0),
                size: 16.0,
                coords: &[],
                synthetic_bold: 0.0,
                synthetic_slant: 0.0,
                brush: zgui_scene::PaintSlot(0),
                glyphs: &GLYPHS,
                text_start: 0,
            });
        }

        fn brush(&self, _brush: zgui_text::Brush) -> Option<zgui_text::TextPaint> {
            None
        }
    }

    let ramp = GradientSpec {
        shape: GradientShape::Linear {
            angle: core::f32::consts::FRAC_PI_2,
        },
        stops: smallvec::smallvec![
            SpecStop {
                color: zgui_color::Color::BLACK,
                position: None,
            },
            SpecStop {
                color: zgui_color::Color::WHITE,
                position: None,
            },
        ],
        interpolation: zgui_color::Interpolation::new(zgui_color::ColorSpace::Srgb),
        repeating: false,
    };
    let style = zgui_paint::lower(&StyleDraft::initial().build(), 1.0);
    let mut scene = Scene::new();
    let mut lone =
        support::chunk::Lone::record(&mut scene, (SpatialId::VIEWPORT, ClipId::ROOT), |rec| {
            text::emit(
                rec,
                &TwoGlyphs,
                ParagraphId(0),
                0,
                &style,
                text::Inherited {
                    text_fill: Some(&ramp),
                    decorations: &[],
                    inline: &[],
                },
                TextPlacement {
                    line: Rect::new(
                        Point::new(DevicePx(20.0), DevicePx(30.0)),
                        Size::new(DevicePx(64.0), DevicePx(20.0)),
                    ),
                    clip: ClipId::ROOT,
                    transform: SpatialId::VIEWPORT,
                    opaque_target: true,
                    subpixel_capable: false,
                    scale: 1.0,
                    ellipsis: None,
                },
            );
        });

    let drawing = lone.redraw();
    let paths: Vec<_> = drawing.of_kind("path").collect();
    assert_eq!(paths.len(), 2, "one filled outline per glyph: {drawing}");
    for path in paths {
        assert_eq!(path.paint("fill").as_deref(), Some("linear"), "{drawing}");
        assert_eq!(
            (path.field("from"), path.field("to")),
            (Some("(20, 40)"), Some("(84, 40)")),
            "the ramp runs across the line box: {drawing}"
        );
    }
}
