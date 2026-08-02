//! The ink-rect audit: every primitive draws inside the rectangle it reports.
//!
//! Under-reporting an ink rectangle is the single most common source of stale pixels, and it is
//! invisible until something moves: a shadow, an outline or a blur that reaches further than the
//! rectangle says leaves a trail behind. So there is one case per primitive kind, and each one
//! draws the decoration that reaches furthest.
//!
//! The audit is against the *fragment's* ink rather than the primitive's own, because that is what
//! damage is computed from — a primitive whose own rectangle is honest and whose fragment's is not
//! is exactly the bug this catches.

mod support;

use support::{Element, Harness};
use zgui_geom::{Device, DevicePx, Rect};
use zgui_paint::{PlacedGlyph, cull_rect};

/// The rectangle every primitive of `name`'s fragment has to sit inside.
fn reported(harness: &Harness, name: &str) -> Rect<DevicePx, Device> {
    let frag = harness.fragment_of(name);
    let fragment = harness.store.fragment(frag).expect("a live fragment");
    fragment
        .ink
        .union(cull_rect(&harness.store, fragment, harness.scale))
}

/// Fails if any primitive in the scene reaches outside `bounds`.
fn assert_all_inside(harness: &Harness, bounds: Rect<DevicePx, Device>, drawn: usize) {
    let primitives = &harness.scene.primitives;
    let mut counted = 0;
    for quad in &primitives.quads {
        assert_contains(bounds, quad.ink(), "quad");
        counted += 1;
    }
    for shadow in &primitives.shadows {
        assert_contains(bounds, shadow.ink(), "shadow");
        counted += 1;
    }
    for decoration in &primitives.decorations {
        assert_contains(bounds, decoration.ink(), "decoration");
        counted += 1;
    }
    for sprite in &primitives.mono_sprites {
        assert_contains(bounds, sprite.ink(), "mono sprite");
        counted += 1;
    }
    for sprite in &primitives.subpixel_sprites {
        assert_contains(bounds, sprite.ink(), "subpixel sprite");
        counted += 1;
    }
    for sprite in &primitives.color_sprites {
        assert_contains(bounds, sprite.ink(), "colour sprite");
        counted += 1;
    }
    for external in &primitives.externals {
        assert_contains(bounds, external.ink(), "external quad");
        counted += 1;
    }
    for group in &primitives.groups {
        assert_contains(bounds, group.source, "group source");
        counted += 1;
    }
    for backdrop in &primitives.backdrops {
        assert_contains(bounds, backdrop.source, "backdrop source");
        counted += 1;
    }
    assert!(
        counted >= drawn,
        "the fixture drew {counted} primitives and the audit expected at least {drawn}: an audit \
         over an empty scene passes while proving nothing"
    );
}

/// Fails if `ink` reaches outside `bounds`.
fn assert_contains(bounds: Rect<DevicePx, Device>, ink: Rect<DevicePx, Device>, kind: &str) {
    assert!(
        bounds.contains_rect(ink),
        "a {kind} at {ink:?} reaches outside the reported ink {bounds:?}"
    );
}

/// A root with one child carrying `css`.
fn one(css: &str) -> Harness {
    let sheet = format!(
        "root {{ display: block; width: 200px; height: 200px }}
         subject {{ display: block; margin: 60px; height: 40px; {css} }}"
    );
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("subject")]),
        Box::leak(sheet.into_boxed_str()),
        200.0,
        200.0,
    );
    harness.paint_everything();
    harness
}

#[test]
fn a_spread_and_blurred_shadow_stays_inside_the_reported_ink() {
    let harness = one("background: #eee; box-shadow: 8px 12px 10px 6px #000");
    assert_all_inside(&harness, reported(&harness, "subject"), 2);
}

#[test]
fn an_inset_shadow_stays_inside_the_box_that_casts_it() {
    let harness = one("background: #eee; box-shadow: inset 8px 12px 10px 6px #000");
    let bounds = reported(&harness, "subject");
    assert_all_inside(&harness, bounds, 2);
    let border_box = harness
        .store
        .fragment(harness.fragment_of("subject"))
        .expect("a fragment")
        .border_box;
    assert_eq!(
        bounds, border_box,
        "an inset shadow paints inside the box, so it must not inflate the ink at all"
    );
}

#[test]
fn an_offset_outline_stays_inside_the_reported_ink() {
    let harness = one("background: #eee; outline: 4px solid #06f; outline-offset: 9px");
    assert_all_inside(&harness, reported(&harness, "subject"), 2);
}

#[test]
fn a_filters_bleed_stays_inside_the_reported_ink() {
    let harness = one("background: #eee; filter: blur(6px)");
    let bounds = reported(&harness, "subject");
    assert_all_inside(&harness, bounds, 2);
    let border_box = harness
        .store
        .fragment(harness.fragment_of("subject"))
        .expect("a fragment")
        .border_box;
    assert!(
        bounds.size.width.0 > border_box.size.width.0,
        "a blur has to have spread the ink, or this case is asserting nothing"
    );
}

#[test]
fn a_backdrop_filters_source_stays_inside_the_reported_ink() {
    let harness = one("backdrop-filter: blur(6px)");
    let bounds = reported(&harness, "subject");
    assert_all_inside(&harness, bounds, 1);
    let border_box = harness
        .store
        .fragment(harness.fragment_of("subject"))
        .expect("a fragment")
        .border_box;
    assert!(
        bounds.size.width.0 > border_box.size.width.0,
        "the read extent has to reach outside the box, or this case is asserting nothing"
    );
}

/// A wavy underline stays inside the line box it decorates.
///
/// The audit above is over box decorations, and a decoration line is the one primitive kind none of
/// them produces: it is emitted for a *line* fragment, from the decorations in force at that line
/// rather than from the line's own style, so the fixture that grows a box out of a style sheet
/// cannot reach it. What stood here instead was arithmetic over a hand-built `DecorationStyle`,
/// which measured neither the emitter nor the ink.
///
/// The band matters because the shader evaluates a wave across the whole rectangle it is given: it
/// has to be several strokes tall to have any amplitude, and it still has to stay inside the line
/// box, because nothing tells the layout stage that an ancestor's decoration is being drawn here.
#[test]
fn a_wavy_underline_is_given_a_band_and_stays_inside_the_line_box() {
    use zgui_css::StyleDraft;
    use zgui_geom::{Point, Rect as GeomRect, Size};
    use zgui_layout::fragment::ParagraphId;
    use zgui_paint::emit::text::{self, DecorationStyle, TextPlacement};
    use zgui_scene::prim::decoration::DecorationStyle as Line;
    use zgui_scene::{ClipId, Scene, SpatialId};

    let line = GeomRect::new(
        Point::new(DevicePx(0.0), DevicePx(100.0)),
        Size::new(DevicePx(64.0), DevicePx(20.0)),
    );
    let placement = TextPlacement {
        line,
        clip: ClipId::ROOT,
        transform: SpatialId::VIEWPORT,
        opaque_target: true,
        subpixel_capable: false,
        upright: true,
        scale: 1.0,
    };
    let wavy = DecorationStyle {
        underline: true,
        overline: true,
        line_through: true,
        style: Line::Wavy,
        thickness: 2.0,
        color: zgui_color::Color::srgb(1.0, 0.0, 0.0, 1.0),
    };

    let mut scene = Scene::new();
    scene.begin_frame(zgui_geom::Size::new(256, 256));
    let style = zgui_paint::lower::lower(&StyleDraft::initial().build(), 1.0);
    let pushed = text::emit(
        &mut scene,
        &zgui_paint::NoGlyphs,
        ParagraphId(0),
        0,
        &style,
        text::Inherited {
            text_fill: None,
            decorations: core::slice::from_ref(&wavy),
        },
        placement,
    );

    assert_eq!(
        pushed, 3,
        "three lines were declared and the emitter drew {pushed}"
    );
    for decoration in &scene.primitives.decorations {
        let ink = decoration.ink();
        assert_contains(line, ink, "decoration");
        assert!(
            ink.size.height.0 > wavy.thickness,
            "a wave squeezed into its own stroke is a straight line: {ink:?}"
        );
    }
}

#[test]
fn a_text_shadow_is_drawn_as_a_second_pass_over_the_same_glyphs_at_its_own_offset() {
    // The shadow copy has to be the same glyph, moved by the offset written and tinted with the
    // shadow's colour — and it has to be drawn *before* the glyph, or it covers what it shadows.
    // This runs the emitter over glyphs of its own, because that is the only way to see where the
    // copy landed rather than where a rectangle would have gone.
    use zgui_atlas::{AtlasTile, TextureId, TextureKind, TileId};
    use zgui_color::Color;
    use zgui_css::StyleDraft;
    use zgui_geom::{Point, Rect as GeomRect, Size};
    use zgui_layout::fragment::ParagraphId;
    use zgui_paint::emit::text::{self, GlyphRun, GlyphSource, TextPlacement};
    use zgui_paint::lower::ShadowSpec;
    use zgui_scene::{ClipId, Scene, SpatialId};
    use zgui_text::GlyphFormat;

    /// One glyph at (10, 10), eight by sixteen.
    struct OneGlyph(PlacedGlyph);

    impl GlyphSource for OneGlyph {
        fn visit_line(
            &self,
            _paragraph: ParagraphId,
            _line: u16,
            _request: zgui_paint::GlyphRequest,
            visit: &mut dyn FnMut(GlyphRun<'_>),
        ) {
            let glyphs = core::slice::from_ref(&self.0);
            visit(GlyphRun {
                content: zgui_paint::RunContent::Tiles(glyphs),
                format: GlyphFormat::Mono,
                paint: zgui_scene::PaintSlot(0),
                synthetic_bold: 0.0,
            });
        }
    }

    let placed = PlacedGlyph {
        resource: AtlasTile {
            texture: TextureId {
                kind: TextureKind::Mono,
                index: 0,
            },
            tile: TileId(0),
            bounds: GeomRect::new(Point::new(0, 0), Size::new(8, 16)),
        }
        .into(),
        bounds: GeomRect::new(
            Point::new(DevicePx(10.0), DevicePx(10.0)),
            Size::new(DevicePx(8.0), DevicePx(16.0)),
        ),
    };
    let mut style = zgui_paint::lower(&StyleDraft::initial().build(), 1.0);
    style.text_shadows.push(ShadowSpec {
        offset_x: 3.0,
        offset_y: 4.0,
        deviation: 0.0,
        spread: 0.0,
        color: Color::srgb(1.0, 0.0, 0.0, 1.0),
        inset: false,
    });

    let mut scene = Scene::new();
    scene.begin_frame(zgui_geom::Size::new(64, 64));
    let pushed = text::emit(
        &mut scene,
        &OneGlyph(placed),
        ParagraphId(0),
        0,
        &style,
        text::Inherited::default(),
        TextPlacement {
            line: GeomRect::new(
                Point::new(DevicePx(0.0), DevicePx(0.0)),
                Size::new(DevicePx(64.0), DevicePx(32.0)),
            ),
            clip: ClipId::ROOT,
            transform: SpatialId::VIEWPORT,
            opaque_target: true,
            subpixel_capable: true,
            upright: true,
            scale: 1.0,
        },
    );
    assert_eq!(pushed, 2, "one shadow copy and one glyph");

    let sprites = &scene.primitives.mono_sprites;
    assert_eq!(sprites.len(), 2, "both copies are the same glyph");
    let shadow = sprites[0];
    let glyph = sprites[1];
    assert!(
        shadow.order < glyph.order,
        "the shadow at {} sorts over the glyph at {}",
        shadow.order,
        glyph.order
    );
    assert_eq!(
        shadow.ink().origin,
        Point::new(DevicePx(13.0), DevicePx(14.0)),
        "the copy moved by the offset written"
    );
    assert_eq!(
        shadow.ink().size,
        glyph.ink().size,
        "a shadow copy is the same glyph"
    );
    assert_ne!(
        shadow.color, glyph.color,
        "the copy is tinted with the shadow's colour rather than the text's"
    );
}

#[test]
fn a_filters_reach_grows_with_the_device_scale() {
    // The read extent is a length, and the two readers of it — the expansion that runs before the
    // walk and the cull that gates the walk — both convert the chain at the frame's own scale. A
    // constant here would under-report every blur on a high-density display by exactly that factor,
    // which is the smearing panel the expansion exists to prevent.
    let harness = one("filter: blur(10px)");
    let frag = harness.fragment_of("subject");
    let at_one = zgui_paint::read_extent_of(&harness.store, frag, 1.0).expect("a read extent");
    let at_two = zgui_paint::read_extent_of(&harness.store, frag, 2.0).expect("a read extent");
    let reach =
        |extent: zgui_paint::ReadExtent| extent.bounds.origin.x.0 - extent.source.origin.x.0;
    assert!(
        reach(at_one) > 0.0,
        "a blur has to reach outside its bounds"
    );
    assert!(
        (reach(at_two) - 2.0 * reach(at_one)).abs() < 1e-3,
        "twice the density is twice the reach: {} against {}",
        reach(at_two),
        reach(at_one)
    );
}

#[test]
fn an_animated_transform_reports_the_union_of_where_it_was_and_where_it_is() {
    // A fragment that moved damages both rectangles, because the pixels it left behind are as stale
    // as the ones it arrived at. The fragment diff is what accumulates that; this asserts it, since
    // the paint stage is what would otherwise be blamed for the trail.
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("subject")]),
        "root { display: block; width: 200px; height: 200px }
         subject { display: block; height: 20px; background: #333 }
         .moved { margin-top: 100px }",
        200.0,
        200.0,
    );
    harness.paint_everything();
    let before = harness
        .store
        .fragment(harness.fragment_of("subject"))
        .expect("a fragment")
        .ink;

    let subject = harness.element("subject");
    harness.edit_and_restyle(|batch| {
        batch.set_classes(subject, &[zgui_interned::ClassName::new("moved")]);
    });
    harness.clear_damage();
    harness.rebuild(200.0, 200.0);

    let after = harness
        .store
        .fragment(harness.fragment_of("subject"))
        .expect("a fragment")
        .ink;
    assert_ne!(
        before.origin, after.origin,
        "the fixture has to have moved it"
    );
    for ink in [before, after] {
        assert!(
            harness
                .damage
                .rects()
                .iter()
                .any(|rect| rect.contains_rect(zgui_layout::fragment::diff::pixels(ink))),
            "{ink:?} is not covered by {:?}",
            harness.damage.rects()
        );
    }
}
