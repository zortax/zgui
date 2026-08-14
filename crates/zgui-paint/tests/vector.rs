//! Drawings reaching the display list through the emit walk.
//!
//! Every assertion here goes through [`Painter::emit`](zgui_paint::Painter::emit) over a real box
//! tree, a real layout pass and real fragments, and reads the result out of a renderer rather than
//! out of the scene the test filled. That is the whole point: a drawing that only reaches the
//! display list when a test calls the emitter itself is a drawing an application cannot draw, and
//! an assertion made by calling the emitter is an assertion that cannot tell the difference.

mod support;

use zgui_atlas::AtlasLimits;
use zgui_bits::DamageSet;
use zgui_paint::VectorCache;
use zgui_scene::Scene;
use zgui_vocab::{PropKey, PropValue, prop::drawing};

use support::{Element, Harness};

/// A root with one drawing in it, sized by its style.
const CSS: &str = "root { display: block; width: 200px; height: 100px }
                   mark { display: block; width: 48px; height: 48px; color: rgb(0, 128, 255) }";

/// A triangle written in a twenty-four unit square.
const TRIANGLE: &str = "M0 0 L24 0 L24 24 Z";

/// The fixture tree: a root with one drawing under it.
fn tree() -> Element {
    Element::new("root").children(vec![
        Element::new("mark").drawing(TRIANGLE, Some("0 0 24 24")),
    ])
}

#[test]
fn an_eligible_drawing_uses_one_tinted_mask_and_no_vector_item() {
    let mut harness = Harness::new(tree(), CSS);
    let vectors = VectorCache::new();
    let mut content = zgui_paint::ContentCache::new(AtlasLimits::default());
    let report = harness.paint_cached_vectors(
        &vectors,
        &mut content,
        &zgui_testkit_scene::MonoRaster::new(),
    );

    assert!(harness.scene().primitives.vectors.is_empty());
    assert_eq!(harness.scene().primitives.mono_sprites.len(), 1);
    assert_eq!(content.report().tiles, 1);
    assert_eq!(report.vector_routes.len(), 1);
    assert!(
        report.vector_routes[0]
            .routes
            .contains(zgui_paint::VectorRoute::AtlasMask)
    );
    assert!(
        !report.vector_routes[0]
            .routes
            .contains(zgui_paint::VectorRoute::GeneralRaster)
    );
}

#[test]
fn recolouring_reuses_a_mask_but_different_geometry_allocates_another() {
    let mut content = zgui_paint::ContentCache::new(AtlasLimits::default());
    let raster = zgui_testkit_scene::MonoRaster::new();

    let mut blue = Harness::new(tree(), CSS);
    blue.paint_cached_vectors(&VectorCache::new(), &mut content, &raster);
    let blue_color = blue.scene().primitives.mono_sprites[0].color;
    assert_eq!(content.report().tiles, 1);

    let red_css = "root { display: block; width: 200px; height: 100px }
                   mark { display: block; width: 48px; height: 48px; color: red }";
    let mut red = Harness::new(tree(), red_css);
    red.paint_cached_vectors(&VectorCache::new(), &mut content, &raster);
    assert_ne!(red.scene().primitives.mono_sprites[0].color, blue_color);
    assert_eq!(content.report().tiles, 1, "tint is not a mask key");

    let small_css = "root { display: block; width: 200px; height: 100px }
                     mark { display: block; width: 16px; height: 16px; color: red }";
    let mut small = Harness::new(tree(), small_css);
    small.paint_cached_vectors(&VectorCache::new(), &mut content, &raster);
    assert_eq!(
        content.report().tiles,
        2,
        "different geometry gets new coverage"
    );
}

#[test]
fn an_eligible_canvas_fill_uses_the_same_mask_route() {
    let handle = zgui_canvas::SceneHandle::new();
    handle.edit(|scene| {
        let path = zgui_scene::kurbo::BezPath::from_svg(TRIANGLE).unwrap();
        scene.push(
            zgui_canvas::ShapeBuilder::new(path)
                .fill(zgui_canvas::Brush::Solid(zgui_color::Color::BLACK))
                .build(),
        );
    });
    let tree = Element::new("root").children(vec![Element::new("mark").canvas(&handle)]);
    let mut harness = Harness::new(tree, CSS);
    let vectors = VectorCache::new();
    let mut content = zgui_paint::ContentCache::new(AtlasLimits::default());
    let report = harness.paint_cached_vectors(
        &vectors,
        &mut content,
        &zgui_testkit_scene::MonoRaster::new(),
    );

    assert!(harness.scene().primitives.vectors.is_empty());
    assert_eq!(harness.scene().primitives.mono_sprites.len(), 1);
    assert!(
        report.vector_routes[0]
            .routes
            .contains(zgui_paint::VectorRoute::AtlasMask)
    );
}

#[test]
fn a_small_solid_canvas_stroke_uses_a_mask_instead_of_the_vector_rasteriser() {
    let handle = zgui_canvas::SceneHandle::new();
    handle.edit(|scene| {
        let mut path = zgui_scene::kurbo::BezPath::new();
        path.move_to((3.0, 12.0));
        path.line_to((21.0, 12.0));
        scene.push(
            zgui_canvas::ShapeBuilder::new(path)
                .stroke(zgui_canvas::Brush::Inherited { alpha: 1.0 }, 2.0)
                .build(),
        );
    });
    let tree = Element::new("root").children(vec![Element::new("mark").canvas(&handle)]);
    let mut harness = Harness::new(tree, CSS);
    let vectors = VectorCache::new();
    let mut content = zgui_paint::ContentCache::new(AtlasLimits::default());
    harness.paint_cached_vectors(
        &vectors,
        &mut content,
        &zgui_testkit_scene::MonoRaster::new(),
    );

    assert!(harness.scene().primitives.vectors.is_empty());
    assert_eq!(harness.scene().primitives.mono_sprites.len(), 1);
    assert_eq!(content.report().tiles, 1);
}

#[test]
fn a_transparent_fill_does_not_disqualify_a_small_solid_css_stroke() {
    let css = "root { display: block; width: 200px; height: 100px }
               mark { display: block; width: 24px; height: 24px;
                      --zgui-fill: transparent; --zgui-stroke: currentColor;
                      --zgui-stroke-width: 2px }";
    let mut harness = Harness::new(tree(), css);
    let vectors = VectorCache::new();
    let mut content = zgui_paint::ContentCache::new(AtlasLimits::default());
    harness.paint_cached_vectors(
        &vectors,
        &mut content,
        &zgui_testkit_scene::MonoRaster::new(),
    );

    assert!(harness.scene().primitives.vectors.is_empty());
    assert_eq!(harness.scene().primitives.mono_sprites.len(), 1);
}

/// A shape carrying both a fill and a stroke is two sprites, which is what it was all along.
///
/// The general rasteriser makes two items of it too — the stroke composited over the fill — so two
/// tinted masks in the same order draw the same picture from the atlas. Each is measured against
/// its own ink, because a stroke puts ink outside the interior it follows.
#[test]
fn a_shape_with_a_fill_and_a_stroke_is_two_sprites_and_no_vector_item() {
    let css = "root { display: block; width: 200px; height: 100px }
               mark { display: block; width: 48px; height: 48px;
                      --zgui-fill: rgb(0, 128, 255); --zgui-stroke: rgb(255, 0, 0);
                      --zgui-stroke-width: 2px }";
    let mut harness = Harness::new(tree(), css);
    let mut content = zgui_paint::ContentCache::new(AtlasLimits::default());
    let report = harness.paint_cached_vectors(
        &VectorCache::new(),
        &mut content,
        &zgui_testkit_scene::MonoRaster::new(),
    );

    assert!(report.vector_routes[0]
        .routes
        .contains(zgui_paint::VectorRoute::AtlasMask));
    assert!(!report.vector_routes[0]
        .routes
        .contains(zgui_paint::VectorRoute::GeneralRaster));
    assert!(harness.scene().primitives.vectors.is_empty());

    let sprites = &harness.scene().primitives.mono_sprites;
    assert_eq!(sprites.len(), 2);
    assert_eq!(content.report().tiles, 2, "the two parts are two rasters");
    let (fill, stroke) = (sprites[0], sprites[1]);
    assert!(
        stroke.order > fill.order,
        "the stroke did not composite over the fill"
    );
    assert!(
        stroke.ink().left() < fill.ink().left() && stroke.ink().right() > fill.ink().right(),
        "the stroke was measured against the interior it follows rather than its own ink"
    );
}

/// Every line the renderer was handed for a vector item.
fn shapes(scene: &Scene) -> Vec<String> {
    let mut renderer = zgui_testkit_scene::CaptureRenderer::new();
    zgui_render::Renderer::draw(&mut renderer, scene, &DamageSet::full());
    renderer
        .transcript()
        .expect("a frame was drawn")
        .to_string()
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("vector order="))
        .map(str::to_owned)
        .collect()
}

/// The whole defect, stated as one test: an element carrying outlines has to reach a rasteriser.
///
/// Before the walk had an arm for it, the fragment was composed, visited, damaged and emitted, and
/// contributed no primitive at all — so every icon in a component library rendered as empty space
/// while every unit test of the emitter passed.
#[test]
fn an_element_carrying_outlines_reaches_a_rasteriser_through_the_emit_walk() {
    let mut harness = Harness::new(tree(), CSS);
    let cache = VectorCache::new();
    let report = harness.paint_vectors(&cache);

    assert!(
        report.primitives > 0,
        "the walk emitted nothing at all for a document whose only content is a drawing"
    );
    assert_eq!(report.vector_routes.len(), 1);
    assert!(
        report.vector_routes[0]
            .routes
            .contains(zgui_paint::VectorRoute::GeneralRaster)
    );
    let shapes = shapes(harness.scene());
    assert_eq!(shapes.len(), 1, "one outline is one shape: {shapes:?}");
    assert!(
        shapes[0].contains("fill=solid srgb(0, 0.502, 1, 1)"),
        "the fill is the element's own computed colour: {}",
        shapes[0]
    );
}

/// A retained canvas reaches the rasteriser the same way, carrying its own per-shape paint.
///
/// This is the whole canvas feature in one walk: the element carries a token, the cache resolves
/// it out of the registry, the shapes keep the fills the application gave them — not the
/// element's colour — and the emit walk hands the rasteriser one item per painted shape.
#[test]
fn a_canvas_scene_reaches_a_rasteriser_with_its_own_paints() {
    let handle = zgui_canvas::SceneHandle::new();
    handle.edit(|scene| {
        let mut path = zgui_scene::kurbo::BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((24.0, 0.0));
        path.line_to((24.0, 24.0));
        path.close_path();
        scene.push(
            zgui_canvas::ShapeBuilder::new(path)
                .fill(zgui_canvas::Brush::Solid(zgui_color::Color::srgb(
                    1.0, 0.0, 0.0, 1.0,
                )))
                .build(),
        );
        let mut line = zgui_scene::kurbo::BezPath::new();
        line.move_to((0.0, 24.0));
        line.line_to((24.0, 0.0));
        scene.push(
            zgui_canvas::ShapeBuilder::new(line)
                .stroke(zgui_canvas::Brush::Inherited { alpha: 1.0 }, 2.0)
                .build(),
        );
    });
    let tree = Element::new("root").children(vec![Element::new("mark").canvas(&handle)]);
    let mut harness = Harness::new(tree, CSS);
    let cache = VectorCache::new();
    let report = harness.paint_vectors(&cache);

    assert!(report.primitives > 0, "the canvas emitted nothing");
    let shapes = shapes(harness.scene());
    assert_eq!(
        shapes.len(),
        2,
        "one filled shape and one stroked shape: {shapes:?}"
    );
    assert!(
        shapes[0].contains("fill=solid srgb(1, 0, 0, 1)"),
        "the first shape keeps its own red, not the element's colour: {}",
        shapes[0]
    );
    assert!(
        shapes[1].contains("srgb(0, 0.502, 1, 1)"),
        "the inherited brush resolves to the element's computed colour: {}",
        shapes[1]
    );
}

/// A drawing is scaled to the box it is drawn in, which is what makes one icon constant serve
/// every size a design system asks for.
#[test]
fn a_drawing_is_fitted_to_the_box_its_style_gave_it() {
    let small = "root { display: block; width: 200px; height: 100px }
                 mark { display: block; width: 16px; height: 16px }";
    let mut harness = Harness::new(tree(), small);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);
    let item = &harness.scene().primitives.vectors[0];
    let bounds = zgui_scene::kurbo::Shape::bounding_box(&*item.path);
    assert_eq!(
        (bounds.width(), bounds.height()),
        (16.0, 16.0),
        "a twenty-four unit outline in a sixteen pixel box is drawn sixteen pixels across"
    );

    let mut harness = Harness::new(tree(), CSS);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);
    let item = &harness.scene().primitives.vectors[0];
    let bounds = zgui_scene::kurbo::Shape::bounding_box(&*item.path);
    assert_eq!((bounds.width(), bounds.height()), (48.0, 48.0));
}

/// The geometry is placed where the box is, not at the origin the outline was written at.
#[test]
fn a_drawing_is_placed_at_its_own_boxs_content_box() {
    let css = "root { display: block; width: 200px; height: 100px; padding: 10px }
               mark { display: block; width: 48px; height: 48px; margin-left: 30px }";
    let mut harness = Harness::new(tree(), css);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);

    let content_box = harness
        .store()
        .fragment(harness.fragment_of("mark"))
        .expect("the drawing produced a fragment")
        .content_box;
    let item = &harness.scene().primitives.vectors[0];
    let bounds = zgui_scene::kurbo::Shape::bounding_box(&*item.path);
    assert_eq!(bounds.x0 as f32, content_box.origin.x.0);
    assert_eq!(bounds.y0 as f32, content_box.origin.y.0);
}

/// The custom-property scheme is what themes a drawing, since this engine build has no `fill`.
#[test]
fn the_fill_and_stroke_custom_properties_reach_the_display_list() {
    let css = "root { display: block; width: 200px; height: 100px;
                      --zgui-fill: rgb(200, 0, 0); --zgui-stroke: rgb(0, 200, 0);
                      --zgui-stroke-width: 3px }
               mark { display: block; width: 48px; height: 48px }";
    let mut harness = Harness::new(tree(), css);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);

    let shapes = shapes(harness.scene());
    assert_eq!(
        shapes.len(),
        2,
        "one filled item and one stroked: {shapes:?}"
    );
    assert!(
        shapes[0].contains("fill=solid srgb(0.7843, 0, 0, 1)"),
        "the fill came from the inherited custom property: {}",
        shapes[0]
    );
    assert!(
        shapes[1].contains("stroke=solid srgb(0, 0.7843, 0, 1)") && shapes[1].contains("width=3"),
        "the stroke came from the inherited custom properties: {}",
        shapes[1]
    );
}

/// A drawing sorts against everything else by the same rule every primitive does, and is clipped by
/// the same chain — an icon inside a scrollport is not a special case in the scene.
#[test]
fn a_drawing_takes_a_draw_order_and_a_clip_like_any_other_primitive() {
    let css = "root { display: block; width: 200px; height: 100px; overflow: hidden;
                      background: rgb(255, 255, 255) }
               mark { display: block; width: 48px; height: 48px }";
    let mut harness = Harness::new(tree(), css);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);

    let scene = harness.scene();
    let item = &scene.primitives.vectors[0];
    let backdrop = scene
        .primitives
        .quads
        .iter()
        .find(|quad| quad.ink().size.width.0 >= 200.0)
        .expect("the root painted its background");
    assert!(
        item.order > backdrop.order,
        "a drawing over a background has to sort over it: {} against {}",
        item.order,
        backdrop.order
    );
    assert_ne!(
        item.clip,
        zgui_scene::ClipId::ROOT,
        "a drawing inside a clipping box is drawn through that box's chain"
    );
    assert_eq!(
        item.clip,
        harness
            .store()
            .fragment(harness.fragment_of("mark"))
            .expect("a fragment")
            .clip,
        "and it is the chain the fragment itself is under, not one of its own"
    );
}

/// Two outlines are two shapes with two identities, so a rasteriser can cache each on its own.
#[test]
fn each_outline_is_its_own_shape_with_its_own_identity() {
    let tree = Element::new("root").children(vec![Element::new("mark").drawing(
        "M0 0 L24 0 L24 24 Z\nM0 0 L0 24 L24 24 Z",
        Some("0 0 24 24"),
    )]);
    let mut harness = Harness::new(tree, CSS);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);

    let items = &harness.scene().primitives.vectors;
    assert_eq!(items.len(), 2);
    assert_ne!(items[0].id, items[1].id);
    assert_ne!(items[0].path, items[1].path, "two marks, drawn twice");
}

/// An element that carries no outlines produces no vector primitive, and does not become one by
/// having a `<vector>`-shaped box.
#[test]
fn an_element_with_no_outlines_draws_nothing() {
    let tree = Element::new("root").children(vec![Element::new("mark")]);
    let mut harness = Harness::new(tree, CSS);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);
    assert!(harness.scene().primitives.vectors.is_empty());
}

/// A control that carries several marks and reveals one is the shape this answers for.
///
/// The mark that is not showing is hidden with `opacity: 0`, which keeps it in the layout so that
/// revealing it moves nothing. It must cost nothing to draw: a drawing is composited from a scratch
/// of its own, so an item nobody can see is a whole rasterisation pass and a rectangle in the damage
/// spent on a shape that composites its target onto itself.
#[test]
fn a_drawing_faded_to_nothing_is_not_in_the_display_list_at_all() {
    let hidden = "root { display: block; width: 200px; height: 100px }
                  mark { display: block; width: 48px; height: 48px; opacity: 0 }";
    let mut harness = Harness::new(tree(), hidden);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);
    assert!(
        harness.scene().primitives.vectors.is_empty(),
        "a drawing every colour of which is fully transparent was still pushed: {:?}",
        shapes(harness.scene())
    );
}

/// Nothing below a vanished box is walked either.
///
/// The primitives were already refused one fragment at a time, which is what the case above
/// states. What a closed disclosure panel costs is its *descendants*: the ink query, the lowered
/// style and the copy composition writes into, the animation lookup and the sorted child list, all
/// spent to arrive at a fragment that pushes nothing. The subtree is refused whole instead, so a
/// panel that is laid out and invisible costs its own box.
#[test]
fn nothing_below_a_vanished_box_is_walked() {
    let hidden = "root { display: block; width: 200px; height: 100px }
                  mark { display: block; width: 48px; height: 48px; opacity: 0 }
                  deep { display: block; width: 48px; height: 48px }";
    let nested = Element::new("root").children(vec![Element::new("mark").children(vec![
        Element::new("deep").drawing(TRIANGLE, Some("0 0 24 24")),
    ])]);
    let mut harness = Harness::new(nested, hidden);
    let cache = VectorCache::new();
    let report = harness.paint_vectors(&cache);

    assert_eq!(
        report.emitted.len(),
        1,
        "the walk descended past a box nothing under it can be seen through"
    );
    assert!(report.skipped_subtrees >= 1);
    assert!(harness.scene().primitives.vectors.is_empty());
    assert!(harness.scene().primitives.mono_sprites.is_empty());
}

/// And the same drawing at an alpha a person can see is still drawn, so the case above is a
/// statement about zero rather than about opacity.
#[test]
fn a_drawing_faded_part_way_is_still_drawn() {
    let faint = "root { display: block; width: 200px; height: 100px }
                 mark { display: block; width: 48px; height: 48px; opacity: 0.25;
                        color: rgb(0, 128, 255) }";
    let mut harness = Harness::new(tree(), faint);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);
    let shapes = shapes(harness.scene());
    assert_eq!(shapes.len(), 1, "the drawing is still there: {shapes:?}");
    assert!(
        shapes[0].contains("fill=solid srgb(0, 0.502, 1, 0.25)"),
        "the group's alpha is folded into the shape's own paint: {}",
        shapes[0]
    );
}

/// An icon swapped for another of the same size moves no geometry, changes no style and stays the
/// same piece of the same box. Without the drawing's own number in the replay record the previous
/// frame's range is replayed and the old icon stays on the screen.
#[test]
fn changing_only_the_outlines_redraws_the_element() {
    let mut harness = Harness::new(tree(), CSS);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);
    let before = harness.scene().primitives.vectors[0].path.to_svg();

    let index = harness.element("mark");
    harness.edit_and_restyle(|edit| {
        edit.set_property(
            index,
            PropKey::new(drawing::PATHS),
            Some(PropValue::from("M12 0 L24 24 L0 24 Z")),
        );
    });
    harness.compose(200.0, 100.0);
    harness.paint_vectors(&cache);

    let after = harness.scene().primitives.vectors[0].path.to_svg();
    assert_ne!(
        before, after,
        "the outlines changed and the frame replayed the ones that were there"
    );
}

/// Changing the outlines has to put the element's own ink in the damage, or the frame that redraws
/// it is scissored to nothing and the change never reaches the screen.
///
/// The composition here reads the document's own marks rather than being told everything is dirty.
/// That is the whole assertion: a pass told everything is dirty re-examines every fragment and
/// produces damage for any change at all, so it cannot tell an invalidation that reached this
/// element from one that never left the property map — which is precisely the state this defect was
/// found in, with a written property and no bit owed for it.
#[test]
fn changing_the_outlines_damages_the_element_that_draws_them() {
    let mut harness = Harness::new(tree(), CSS);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);
    // A first build owes every phase on every node, and nothing has read those marks out of the
    // document yet. Composing once from the marks is what retires them, so that what follows is a
    // settled document in which the only obligation outstanding is the one the change creates.
    harness.compose_from_marks(200.0, 100.0);
    harness.clear_damage();

    let index = harness.element("mark");
    harness.edit_and_restyle(|edit| {
        edit.set_property(
            index,
            PropKey::new(drawing::PATHS),
            Some(PropValue::from("M12 0 L24 24 L0 24 Z")),
        );
    });
    harness.compose_from_marks(200.0, 100.0);

    let ink = harness
        .store()
        .fragment(harness.fragment_of("mark"))
        .expect("a fragment")
        .ink;
    let damage = &harness.damage;
    assert!(
        !damage.is_full(),
        "an icon swapped for another of the same size damaged the whole surface: {damage:?}"
    );
    assert!(
        damage.intersects(zgui_layout::fragment::diff::pixels(ink)),
        "an icon that changed put nothing of its own in the damage: {damage:?}"
    );
}

/// A property nothing draws changes no pixels, which is what makes the case above mean anything.
///
/// Without this, "the outlines are in the damage" is satisfied by a document that damages the
/// element for every write it receives, and the mark the fix added would be doing no work that a
/// blanket invalidation was not already doing.
#[test]
fn changing_a_property_that_draws_nothing_damages_nothing() {
    let mut harness = Harness::new(tree(), CSS);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);
    harness.compose_from_marks(200.0, 100.0);
    harness.clear_damage();

    let index = harness.element("mark");
    harness.edit_and_restyle(|edit| {
        edit.set_property(
            index,
            PropKey::new("data-label"),
            Some(PropValue::from("a name a reader reads")),
        );
    });
    harness.compose_from_marks(200.0, 100.0);

    assert!(
        harness.damage.is_empty(),
        "a property no stage paints damaged pixels: {:?}",
        harness.damage
    );
}

/// A drawing with no space of its own is in the coordinates its own box was placed with, which is
/// what a chart mark needs: the same numbers decided where the box goes.
#[test]
fn a_drawing_with_no_view_box_is_drawn_in_css_pixels_from_its_content_box() {
    let tree = Element::new("root").children(vec![
        Element::new("mark").drawing("M0 0 L10 0 L10 10 Z", None),
    ]);
    let mut harness = Harness::new(tree, CSS);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);

    let content_box = harness
        .store()
        .fragment(harness.fragment_of("mark"))
        .expect("a fragment")
        .content_box;
    let bounds =
        zgui_scene::kurbo::Shape::bounding_box(&*harness.scene().primitives.vectors[0].path);
    assert_eq!(
        (bounds.width(), bounds.height()),
        (10.0, 10.0),
        "ten units is ten CSS pixels, not the whole box"
    );
    assert_eq!(bounds.x0 as f32, content_box.origin.x.0);
}

/// A drawing nothing changed about is still drawn on the frame after, and on every frame after
/// that.
///
/// The frames after the first replay the drawing's chunk, and the replay re-pushes its vector
/// item into the pass planning the scene rebuilds every frame. A replay that skipped it would
/// re-emit nothing while the damage that reached the fragment had already cleared its pixels:
/// the icon disappears, and because the following frames no longer damage the hole it stays gone
/// until something repaints the whole surface. This is the regression this test exists to catch.
#[test]
fn an_unchanged_drawing_is_drawn_again_on_every_frame_that_reaches_it() {
    let mut harness = Harness::new(tree(), CSS);
    let cache = VectorCache::new();

    for frame in 1..=4 {
        harness.paint_vectors(&cache);
        assert_eq!(
            harness.scene().primitives.vectors.len(),
            1,
            "frame {frame} reached the drawing and drew nothing for it"
        );
        assert_eq!(
            shapes(harness.scene()).len(),
            1,
            "frame {frame} handed the rasteriser no outline"
        );
    }
}

/// A root large enough to hold a drawing at the mask limit and one past it.
const LARGE_ROOT: &str = "root { display: block; width: 400px; height: 400px }";

/// The fixture tree with the mark sized to `edge` device pixels square.
fn sized_tree(edge: u32) -> (Element, String) {
    let css = format!(
        "{LARGE_ROOT}
         mark {{ display: block; width: {edge}px; height: {edge}px; color: rgb(0, 128, 255) }}"
    );
    (tree(), css)
}

/// The route a drawing of `edge` square takes.
fn route_at(edge: u32) -> (zgui_paint::VectorRoutes, usize, usize) {
    let (tree, css) = sized_tree(edge);
    routes_of(tree, css.as_str())
}

/// The route one fixture's drawing takes, with the sprites and vector items it produced.
fn routes_of(tree: Element, css: &str) -> (zgui_paint::VectorRoutes, usize, usize) {
    let mut harness = Harness::new(tree, css);
    let report = harness.paint_cached_vectors(
        &VectorCache::new(),
        &mut zgui_paint::ContentCache::new(AtlasLimits::default()),
        &zgui_testkit_scene::MonoRaster::new(),
    );
    (
        report.vector_routes[0].routes,
        harness.scene().primitives.mono_sprites.len(),
        harness.scene().primitives.vectors.len(),
    )
}

/// The route the fixture drawing takes under `transform`.
fn route_under(transform: &str) -> (zgui_paint::VectorRoutes, usize, usize) {
    let css = format!(
        "{LARGE_ROOT}
         mark {{ display: block; width: 48px; height: 48px; color: rgb(0, 128, 255);
                 transform: {transform} }}"
    );
    routes_of(tree(), css.as_str())
}

/// Whether one route is the atlas mask and nothing else.
fn is_mask(result: (zgui_paint::VectorRoutes, usize, usize)) -> bool {
    let (routes, sprites, vectors) = result;
    routes.contains(zgui_paint::VectorRoute::AtlasMask)
        && !routes.contains(zgui_paint::VectorRoute::GeneralRaster)
        && sprites == 1
        && vectors == 0
}

#[test]
fn a_drawing_at_the_mask_limit_still_uses_the_atlas_mask() {
    let (routes, sprites, vectors) = route_at(256);
    assert!(routes.contains(zgui_paint::VectorRoute::AtlasMask));
    assert!(!routes.contains(zgui_paint::VectorRoute::GeneralRaster));
    assert_eq!(sprites, 1);
    assert!(vectors == 0);
}

#[test]
fn a_drawing_over_the_mask_limit_takes_the_general_raster() {
    let (routes, sprites, vectors) = route_at(257);
    assert!(routes.contains(zgui_paint::VectorRoute::GeneralRaster));
    assert!(!routes.contains(zgui_paint::VectorRoute::AtlasMask));
    assert_eq!(sprites, 0);
    assert_eq!(vectors, 1);
}

/// The turns and scales an interface is actually built from stay on the atlas.
///
/// A grip rotated a quarter turn, a chevron turned over, a panel that grows into place: each of
/// these used to be the shape that built a path rasteriser, at a third of a second and a hundred
/// and seventy megabytes, for a picture the atlas draws. The monochrome pages are sampled without
/// filtering, so what makes these exact is that every one of them puts the shape's own axes back on
/// the device's.
#[test]
fn a_drawing_under_a_turn_or_a_scale_stays_on_the_atlas() {
    for transform in [
        "rotate(90deg)",
        "rotate(-90deg)",
        "rotate(180deg)",
        "scale(0.9)",
        "scale(2)",
        "scale(-1, 1)",
        "scaleY(-1)",
        "translate(3px, 5px) rotate(90deg)",
        "rotate(90deg) rotate(90deg) rotate(180deg)",
    ] {
        assert!(
            is_mask(route_under(transform)),
            "`transform: {transform}` was sent to the general rasteriser"
        );
    }
}

/// The sprite one mask arrives as, and the texels behind it.
fn sprite_under(transform: &str) -> (zgui_geom::Rect<zgui_geom::DevicePx, zgui_geom::Device>, [i32; 2]) {
    let css = format!(
        "{LARGE_ROOT}
         mark {{ display: block; width: 48px; height: 48px; color: rgb(0, 128, 255);
                 transform: {transform} }}"
    );
    let mut harness = Harness::new(tree(), css.as_str());
    harness.paint_cached_vectors(
        &VectorCache::new(),
        &mut zgui_paint::ContentCache::new(AtlasLimits::default()),
        &zgui_testkit_scene::MonoRaster::new(),
    );
    let sprite = harness.scene().primitives.mono_sprites[0];
    (sprite.ink(), [sprite.tile.bounds[2], sprite.tile.bounds[3]])
}

/// What the density is for, stated as the two cases it distinguishes.
///
/// The sprite is always the shape's own rectangle, because it rides the same transform every other
/// primitive of the box rides. What moves is the coverage behind it: a turn needs none, so the tile
/// a shape has at rest is the tile it has at every angle; a scale needs twice the texels, and gets
/// them from the rasteriser rather than from a sampler stretching the tile it had.
#[test]
fn a_turn_reuses_a_shapes_texels_and_a_scale_asks_for_more() {
    let (rest, at_rest) = sprite_under("none");
    let (turned, when_turned) = sprite_under("rotate(90deg)");
    assert_eq!(turned, rest, "a turn moved the sprite out of its own box");
    assert_eq!(when_turned, at_rest, "a turn rasterised the outline again");

    let (scaled, when_scaled) = sprite_under("scale(2)");
    assert_eq!(scaled, rest, "a scale moved the sprite out of its own box");
    assert_eq!(
        when_scaled,
        [at_rest[0] * 2, at_rest[1] * 2],
        "a doubled shape was drawn from the texels it had at rest"
    );
}

/// And a map that does not put those axes back stays where it was.
///
/// The bound on the residue is relative, so a quarter turn composed through three transforms is
/// still read as one. It has to refuse a real rotation just as reliably.
#[test]
fn a_drawing_under_a_rotation_or_a_shear_takes_the_general_raster() {
    for transform in ["rotate(30deg)", "rotate(1deg)", "skewX(20deg)"] {
        let (routes, sprites, vectors) = route_under(transform);
        assert!(
            routes.contains(zgui_paint::VectorRoute::GeneralRaster),
            "`transform: {transform}` was drawn from an unfiltered mask"
        );
        assert_eq!(sprites, 0);
        assert_eq!(vectors, 1);
    }
}

/// A stroke has one width whatever direction the outline runs in, and one mask cannot say that
/// under a map that scales the two axes differently.
#[test]
fn a_stroke_under_a_non_uniform_scale_takes_the_general_raster() {
    let stroked = "root { display: block; width: 400px; height: 400px }
                   mark { display: block; width: 48px; height: 48px; color: transparent;
                          --zgui-stroke: rgb(0, 128, 255); --zgui-stroke-width: 2px;
                          transform: scale(2, 1) }";
    let (routes, sprites, vectors) = routes_of(tree(), stroked);
    assert!(routes.contains(zgui_paint::VectorRoute::GeneralRaster));
    assert_eq!(sprites, 0);
    // The fill and the stroke, which is what the general path pushes for a shape carrying both,
    // whatever either of them is coloured.
    assert_eq!(vectors, 2);

    let uniform = stroked.replace("scale(2, 1)", "scale(2)");
    assert!(is_mask(routes_of(tree(), uniform.as_str())));
}

/// A drawing scaled to nothing is drawn by nobody.
#[test]
fn a_drawing_scaled_to_nothing_reaches_neither_rasteriser() {
    let css = format!(
        "{LARGE_ROOT}
         mark {{ display: block; width: 48px; height: 48px; color: rgb(0, 128, 255);
                 transform: scale(0) }}"
    );
    let mut harness = Harness::new(tree(), css.as_str());
    harness.paint_cached_vectors(
        &VectorCache::new(),
        &mut zgui_paint::ContentCache::new(AtlasLimits::default()),
        &zgui_testkit_scene::MonoRaster::new(),
    );
    assert!(harness.scene().primitives.vectors.is_empty());
    assert!(harness.scene().primitives.mono_sprites.is_empty());
}

/// A bar with no view box, `width` by `height` CSS pixels, filled.
fn bar_tree(width: u32, height: u32) -> (Element, String) {
    let path: &'static str = Box::leak(
        format!("M0 0 L{width} 0 L{width} {height} L0 {height} Z").into_boxed_str(),
    );
    let css = format!(
        "root {{ display: block; width: 1400px; height: 1400px }}
         mark {{ display: block; width: {width}px; height: {height}px; color: rgb(0, 128, 255) }}"
    );
    (
        Element::new("root").children(vec![Element::new("mark").drawing(path, None)]),
        css,
    )
}

/// The budget is stated in texels and in each edge, so a rule takes the atlas and a square does not.
///
/// An axis, a divider and a gridline are all far longer than any icon and hold less coverage than
/// one. Under a cap on the longest edge every one of them built a path rasteriser.
#[test]
fn a_wide_thin_drawing_takes_the_atlas_and_a_large_square_does_not() {
    let (tree, css) = bar_tree(1024, 2);
    assert!(is_mask(routes_of(tree, css.as_str())), "a 1024×2 rule");

    for (width, height, why) in [
        (1025, 2, "one texel wider than a page"),
        (2, 1024, "a shelf nothing shorter will be placed in"),
        (300, 300, "ninety thousand texels"),
    ] {
        let (tree, css) = bar_tree(width, height);
        let (routes, _, vectors) = routes_of(tree, css.as_str());
        assert!(
            routes.contains(zgui_paint::VectorRoute::GeneralRaster),
            "{width}×{height} was put in the atlas, and it is {why}"
        );
        assert_eq!(vectors, 1);
    }
}
