//! What a real document actually draws, for two declarations that drew nothing at all.
//!
//! Both defects here passed every test in the workspace. They could, because every assertion about
//! opacity was written over a hand-built scene and every assertion about text decoration was
//! written over a hand-built [`DecorationStyle`], and in both cases the thing that was broken sat
//! *between* the style engine and the display list — which is exactly the span no hand-built input
//! crosses. So these mount a document, style it with the declaration in question, and ask what the
//! window shows.
//!
//! Each case carries the control that stops it passing while the feature is dead. A translucent box
//! is asserted to be *present*, at the alpha it was given, and beside a sibling that composites
//! into a target of its own — because the failure was a primitive being swept into that sibling's
//! target and clipped away, and a case with no such sibling never sees it. A decorated line is
//! asserted to draw a decoration line, in the colour the sheet named, over the line box the
//! text landed in — because the failure was no primitive at all, and "the style says line-through"
//! is a claim about the cascade rather than about the frame.
//!
//! [`DecorationStyle`]: zgui_paint::emit::text::DecorationStyle

mod support;

use zgui_testkit_compositor::{Drawing, Instance};
use zgui_view::{Anchor, BuildCx, IntoView, View};

/// Mounts a document styled by `css` and returns what its window shows once it settles.
fn frames(
    css: &'static str,
    view: impl FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
) -> Drawing {
    let mut harness = support::app_with_text(css, view);
    harness.settle(4);
    support::drawing(&harness.app().windows()[0])
}

/// The decoration lines of `drawing`, in draw order.
fn lines(drawing: &Drawing) -> Vec<&Instance> {
    drawing.of_kind("line").collect()
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
    let quads = drawn.quads();

    let opaque = quads
        .iter()
        .find(|quad| quad.color("fill").is_some_and(|[.., alpha]| alpha >= 1.0))
        .expect("the filtered swatch is missing, so there is nothing to compare against");
    let faded = quads
        .iter()
        .find(|quad| {
            // The compositor carries the alpha in eight bits.
            quad.color("fill")
                .is_some_and(|[.., alpha]| (alpha - 0.45).abs() <= 1.0 / 255.0)
        })
        .unwrap_or_else(|| panic!("`opacity: 0.45` drew no quad at all:\n{drawn}"));
    let faded = support::world(faded);
    assert_eq!(
        faded[2..],
        [40.0, 40.0],
        "the translucent box is drawn at its own size"
    );
    assert!(
        faded[0] > support::world(opaque)[0],
        "it is drawn where the layout put it, beside the sibling rather than under it"
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
    let lines = lines(&drawn);

    assert_eq!(
        lines.len(),
        1,
        "one decorated element and one undecorated one draw exactly one line:\n{drawn}"
    );
    let line = lines[0];
    assert_eq!(
        line.color("color"),
        Some([1.0, 0.0, 0.0, 1.0]),
        "the line is drawn in `text-decoration-color`, not in the text's own colour"
    );
    let bounds = support::world(line);
    assert!(
        bounds[2] > 0.0 && bounds[3] > 0.0,
        "a line with no extent draws nothing: {line:?}"
    );
    // A strike sits inside the line box it crosses rather than under it, which is what says the
    // line was placed against the text rather than at some fixed offset from the box.
    assert!(
        bounds[1] > 0.0 && bounds[1] < 24.0,
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
    let lines = lines(&drawn);
    assert_eq!(lines.len(), 1, "{drawn}");
    assert_eq!(
        lines[0].color("color"),
        Some([0.0, 0.0, 1.0, 1.0]),
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
    let lines = lines(&drawn);
    assert_eq!(lines.len(), 2, "{drawn}");

    let wavy = lines
        .iter()
        .find(|line| line.field("style") == Some("wavy"))
        .expect("`text-decoration: underline wavy` produced no wavy line");
    let solid = lines
        .iter()
        .find(|line| line.field("style") == Some("solid"))
        .expect("the solid control is missing, so the comparison below proves nothing");
    let (wavy, solid) = (support::world(wavy), support::world(solid));

    assert!(
        wavy[3] > solid[3],
        "a wave needs more height than a stroke or it has no amplitude: wavy {wavy:?} solid \
         {solid:?}"
    );
    assert!(
        wavy[3] >= solid[3] * 3.0,
        "the band is three strokes tall, which is what the shader's wave and its doubled line are \
         written against: {wavy:?}"
    );
}

/// The sheet the nested inline case is styled by: a paragraph away from the window's corner, and a
/// box inside its text with a shadow and an outline.
const NESTED_CSS: &str = "root { display: block; width: 400px; height: 300px }
     .para { display: block; margin-left: 100px; margin-top: 60px }
     .hot { box-shadow: 0 0 0 3px #ff00ff; outline: 2px solid #00ff00 }";

#[test]
fn a_nested_inline_box_draws_its_shadow_and_outline_around_its_text() {
    // The box has no rectangle of its own size, so a shadow or an outline drawn from its own
    // fragment lands at the start of the paragraph's content, around nothing.
    let drawn = frames(NESTED_CSS, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::text()
                        .class("para")
                        .child(|| "aa ")
                        .child(zgui_elements::text().class("hot").child(|| "bbbb")),
                )
                .into_view()
                .build(cx),
        )
    });
    let shadows: Vec<[f32; 4]> = drawn
        .of_kind("shadow")
        .map(|shadow| {
            shadow
                .rect("caster")
                .unwrap_or_else(|| support::world(shadow))
        })
        .collect();

    assert!(
        shadows
            .iter()
            .all(|bounds| bounds[2] > 0.0 && bounds[3] > 0.0),
        "a shadow is cast from a rectangle with no extent: {shadows:?}"
    );
    assert_eq!(shadows.len(), 1, "{drawn}");
    let text = shadows[0];
    assert!(
        text[0] > 100.0,
        "the shadow is cast from the box's text, past `aa `: {text:?}"
    );

    let unfilled: Vec<[f32; 4]> = drawn
        .quads()
        .into_iter()
        .filter(|quad| quad.field("fill") == Some("none"))
        .map(support::world)
        .collect();
    let outlines: Vec<_> = unfilled
        .iter()
        .filter(|quad| quad[2] > 0.0)
        .filter(|quad| quad[0] < text[0] && quad[0] + quad[2] > text[0])
        .collect();
    assert!(
        unfilled
            .iter()
            .all(|quad| !(quad[0] < 100.0 && quad[0] + quad[2] <= 104.0 && quad[1] < 64.0)),
        "an outline is drawn at the start of the paragraph:\n{drawn}"
    );
    assert_eq!(
        outlines.len(),
        1,
        "one outline around the box's text:\n{drawn}"
    );
}
