//! Drawings reaching the compositor through the emit walk.
//!
//! Every assertion here goes through [`Painter::emit`](zgui_paint::Painter::emit) over a real box
//! tree, a real layout pass and real fragments, and reads the result out of a compositor frame
//! rather than out of the scene the test filled. That is the whole point: a drawing that only
//! reaches the compositor when a test calls the emitter itself is a drawing an application cannot
//! draw, and an assertion made by calling the emitter is an assertion that cannot tell the
//! difference.
//!
//! The compositor draws a small path with solid paints as mono masks and any other as a path, so
//! a shape is either kind of instance.

mod support;

use zgui_paint::Tables;
use zgui_paint::VectorCache;
use zgui_paint::emit::record::Recording;
use zgui_testkit_compositor::{Drawing, Instance};
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

/// Paints one frame through a content cache, so the shapes take the routes a window gives them.
fn paint_cached(harness: &mut Harness) -> zgui_paint::PaintReport {
    harness.paint_cached_vectors(&VectorCache::new(), &mut zgui_paint::ContentCache::new())
}

/// The mono masks and paths of `drawing`, in draw order.
fn shapes(drawing: &Drawing) -> Vec<&Instance> {
    drawing
        .instances()
        .iter()
        .filter(|instance| matches!(instance.kind(), "mono" | "path"))
        .collect()
}

/// The colour a mono mask or a path fills with.
fn color(shape: &Instance) -> Option<[f32; 4]> {
    shape.color("paint").or_else(|| shape.color("fill"))
}

/// Whether two colours are within one eight-bit step of each other.
fn near(found: Option<[f32; 4]>, wanted: [f32; 4]) -> bool {
    found.is_some_and(|found| {
        found
            .iter()
            .zip(wanted)
            .all(|(found, wanted)| (found - wanted).abs() <= 1.0 / 255.0)
    })
}

/// Records the drawing of the element `mark` in a chunk of its own, on the general route, with
/// the paint its style gives it.
fn recorded(harness: &Harness, cache: &VectorCache) -> Recording {
    let fragment = harness
        .store()
        .fragment(harness.fragment_of("mark"))
        .expect("the drawing produced a fragment");
    let node = harness.document().store().key_of(harness.element("mark"));
    let drawing = zgui_paint::VectorSource::drawing(
        &cache.frame(harness.document()),
        node,
        zgui_paint::content::vectors::Placement {
            content_box: fragment.content_box(),
            scale: 1.0,
        },
    )
    .expect("the element draws");
    let style = &harness.store().node(fragment.box_).style;
    let mut scene = Tables::new();
    scene.begin_frame(zgui_geom::Size::new(200, 100));
    let mut recording = Recording::new();
    let mut rec = recording.recorder(
        &mut scene,
        zgui_space::SpatialId::VIEWPORT,
        zgui_space::ClipId::ROOT,
    );
    zgui_paint::emit::vector::draw_drawing(
        &mut rec,
        zgui_paint::VectorId(1),
        &drawing,
        zgui_paint::emit::vector::shape_paint(style, 1.0),
        &zgui_paint::content::NoVectorMasks,
        zgui_paint::emit::vector::VectorPlacement {
            clip: zgui_space::ClipId::ROOT,
            transform: zgui_space::SpatialId::VIEWPORT,
            scale: 1.0,
        },
    );
    recording
}

#[test]
fn an_eligible_drawing_is_one_path_the_compositor_draws_as_one_mask() {
    let mut harness = Harness::new(tree(), CSS);
    let report = paint_cached(&mut harness);

    assert_eq!(report.vector_routes.len(), 1);
    assert!(
        report.vector_routes[0]
            .routes
            .contains(zgui_paint::VectorRoute::GeneralRaster)
    );
    let drawing = harness.redraw();
    let shapes = shapes(&drawing);
    let [mask] = shapes[..] else {
        panic!("{drawing}");
    };
    assert_eq!(mask.kind(), "mono", "{drawing}");
    assert!(near(color(mask), [0.0, 0.502, 1.0, 1.0]), "{drawing}");
}

#[test]
fn an_eligible_canvas_fill_is_one_mask_as_well() {
    let handle = zgui_canvas::SceneHandle::new();
    handle.edit(|scene| {
        let path = kurbo::BezPath::from_svg(TRIANGLE).unwrap();
        scene.push(
            zgui_canvas::ShapeBuilder::new(path)
                .fill(zgui_canvas::Brush::Solid(zgui_color::Color::BLACK))
                .build(),
        );
    });
    let tree = Element::new("root").children(vec![Element::new("mark").canvas(&handle)]);
    let mut harness = Harness::new(tree, CSS);
    let report = paint_cached(&mut harness);

    assert!(
        report.vector_routes[0]
            .routes
            .contains(zgui_paint::VectorRoute::GeneralRaster)
    );
    let drawing = harness.redraw();
    assert_eq!(drawing.of_kind("mono").count(), 1, "{drawing}");
    assert_eq!(drawing.of_kind("path").count(), 0, "{drawing}");
}

#[test]
fn a_small_solid_canvas_stroke_is_one_mask() {
    let handle = zgui_canvas::SceneHandle::new();
    handle.edit(|scene| {
        // A curve, because a straight one is a quad on the analytic route or a mark.
        let mut path = kurbo::BezPath::new();
        path.move_to((3.0, 3.0));
        path.curve_to((9.0, 3.0), (15.0, 21.0), (21.0, 21.0));
        scene.push(
            zgui_canvas::ShapeBuilder::new(path)
                .stroke(zgui_canvas::Brush::Inherited { alpha: 1.0 }, 2.0)
                .build(),
        );
    });
    let tree = Element::new("root").children(vec![Element::new("mark").canvas(&handle)]);
    let mut harness = Harness::new(tree, CSS);
    paint_cached(&mut harness);

    let drawing = harness.redraw();
    assert_eq!(drawing.of_kind("mono").count(), 1, "{drawing}");
    assert_eq!(drawing.of_kind("path").count(), 0, "{drawing}");
}

#[test]
fn a_transparent_fill_does_not_disqualify_a_small_solid_css_stroke() {
    let css = "root { display: block; width: 200px; height: 100px }
               mark { display: block; width: 24px; height: 24px;
                      --zgui-fill: transparent; --zgui-stroke: currentColor;
                      --zgui-stroke-width: 2px }";
    let mut harness = Harness::new(tree(), css);
    paint_cached(&mut harness);

    let drawing = harness.redraw();
    assert_eq!(drawing.of_kind("mono").count(), 1, "{drawing}");
    assert_eq!(drawing.of_kind("path").count(), 0, "{drawing}");
}

/// A shape carrying both a fill and a stroke is one path, which the compositor draws as two
/// masks, the stroke over the fill. Each mask is measured against its own ink, because a stroke
/// puts ink outside the interior it follows.
#[test]
fn a_shape_with_a_fill_and_a_stroke_is_one_path_and_two_masks() {
    let css = "root { display: block; width: 200px; height: 100px }
               mark { display: block; width: 48px; height: 48px;
                      --zgui-fill: rgb(0, 128, 255); --zgui-stroke: rgb(255, 0, 0);
                      --zgui-stroke-width: 2px }";
    let mut harness = Harness::new(tree(), css);
    let report = paint_cached(&mut harness);

    assert!(
        report.vector_routes[0]
            .routes
            .contains(zgui_paint::VectorRoute::GeneralRaster)
    );
    assert_eq!(
        harness.compositor.commit.chunks_recorded, 1,
        "the drawing, and no chunk for the root, which draws nothing"
    );
    let drawing = harness.redraw();
    let shapes = shapes(&drawing);
    let [fill, stroke] = shapes[..] else {
        panic!("{drawing}");
    };
    assert!(near(color(fill), [0.0, 0.502, 1.0, 1.0]), "{drawing}");
    assert!(
        near(color(stroke), [1.0, 0.0, 0.0, 1.0]),
        "the stroke draws over the fill: {drawing}"
    );
    let (fill, stroke) = (fill.bounds(), stroke.bounds());
    assert!(
        stroke[0] < fill[0] && stroke[0] + stroke[2] > fill[0] + fill[2],
        "the stroke was measured against its own ink: {drawing}"
    );
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
    let drawing = harness.redraw();
    let shapes = shapes(&drawing);
    assert_eq!(shapes.len(), 1, "one outline is one shape: {drawing}");
    assert!(
        near(color(shapes[0]), [0.0, 0.502, 1.0, 1.0]),
        "the fill is the element's own computed colour: {drawing}"
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
        let mut path = kurbo::BezPath::new();
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
        let mut line = kurbo::BezPath::new();
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
    let drawing = harness.redraw();
    let shapes = shapes(&drawing);
    assert_eq!(
        shapes.len(),
        2,
        "one filled shape and one stroked shape: {drawing}"
    );
    assert!(
        near(color(shapes[0]), [1.0, 0.0, 0.0, 1.0]),
        "the first shape keeps its own red, not the element's colour: {drawing}"
    );
    assert!(
        near(color(shapes[1]), [0.0, 0.502, 1.0, 1.0]),
        "the inherited brush resolves to the element's computed colour: {drawing}"
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
    let bounds = recorded_bounds(&harness, &cache);
    assert_eq!(
        (bounds.width(), bounds.height()),
        (16.0, 16.0),
        "a twenty-four unit outline in a sixteen pixel box is drawn sixteen pixels across"
    );
    assert_eq!(shapes(&harness.redraw()).len(), 1);

    let mut harness = Harness::new(tree(), CSS);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);
    let bounds = recorded_bounds(&harness, &cache);
    assert_eq!((bounds.width(), bounds.height()), (48.0, 48.0));
}

/// The bounds of the one path the drawing of `mark` records, placed in the fragment's space.
fn recorded_bounds(harness: &Harness, cache: &VectorCache) -> kurbo::Rect {
    use kurbo::Shape as _;

    let recording = recorded(harness, cache);
    let [item] = recording.paths() else {
        panic!("{} paths", recording.paths().len());
    };
    (item.placement * item.path.as_ref().clone()).bounding_box()
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
        .content_box();
    let drawing = harness.redraw();
    let [mask] = shapes(&drawing)[..] else {
        panic!("{drawing}");
    };
    let bounds = mask.bounds();
    assert_eq!(bounds[0], content_box.origin.x.0, "{drawing}");
    assert_eq!(bounds[1], content_box.origin.y.0, "{drawing}");
}

/// The general route draws the drawing's own source path, with the fit as the item's placement.
#[test]
fn a_general_shape_draws_its_source_path_under_the_fit() {
    let mut harness = Harness::new(tree(), CSS);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);

    let fragment = harness
        .store()
        .fragment(harness.fragment_of("mark"))
        .expect("the drawing produced a fragment");
    let node = harness.document().store().key_of(harness.element("mark"));
    let drawing = zgui_paint::VectorSource::drawing(
        &cache.frame(harness.document()),
        node,
        zgui_paint::content::vectors::Placement {
            content_box: fragment.content_box(),
            scale: 1.0,
        },
    )
    .expect("the element draws");
    let recording = recorded(&harness, &cache);
    let item = &recording.paths()[0];
    assert!(
        std::sync::Arc::ptr_eq(&item.path, &drawing.shapes[0].path),
        "the item draws the source path, not a placed copy"
    );
    assert_eq!(item.placement, drawing.fit);
}

/// An inherited stroke is a width in device pixels, so the fit that scales the path does not
/// scale it.
#[test]
fn an_inherited_stroke_keeps_its_device_width_under_a_fit() {
    let css = "root { display: block; width: 200px; height: 100px;
                      --zgui-stroke: rgb(0, 200, 0); --zgui-stroke-width: 3px }
               mark { display: block; width: 48px; height: 48px }";
    let mut harness = Harness::new(tree(), css);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);

    let recording = recorded(&harness, &cache);
    let item = recording
        .paths()
        .iter()
        .find(|item| item.stroke.is_some())
        .expect("a stroked item");
    let scale = item.placement.determinant().abs().sqrt();
    assert_eq!(scale, 2.0, "a 24 unit drawing in a 48 px box");
    let width = item.stroke.as_ref().expect("a stroke").style.width * scale;
    assert!(
        (width - 3.0).abs() < 1.0e-9,
        "the stroke is {width} px wide on the device"
    );
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

    let drawing = harness.redraw();
    let shapes = shapes(&drawing);
    assert_eq!(
        shapes.len(),
        2,
        "one filled mask and one stroked: {drawing}"
    );
    assert!(
        near(color(shapes[0]), [0.7843, 0.0, 0.0, 1.0]),
        "the fill came from the inherited custom property: {drawing}"
    );
    assert!(
        near(color(shapes[1]), [0.0, 0.7843, 0.0, 1.0]),
        "the stroke came from the inherited custom properties: {drawing}"
    );
    let recording = recorded(&harness, &cache);
    let stroke = recording.paths()[0].stroke.as_ref().expect("a stroke");
    assert_eq!(
        stroke.style.width * 2.0,
        3.0,
        "three pixels under a fit of two"
    );
}

/// A drawing is clipped by the same chain as everything else its fragment draws — an icon inside a
/// scrollport is not a special case in the scene.
#[test]
fn a_drawing_is_clipped_like_any_other_primitive() {
    let css = "root { display: block; width: 200px; height: 100px; overflow: hidden;
                      background: rgb(255, 255, 255) }
               mark { display: block; width: 48px; height: 48px }";
    let mut harness = Harness::new(tree(), css);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);

    let drawing = harness.redraw();
    let [mask] = shapes(&drawing)[..] else {
        panic!("{drawing}");
    };
    let at = support::poison();
    assert_eq!(
        mask.field("clip"),
        Some(format!("[rect({at}, {at}, 200, 100)]").as_str()),
        "a drawing inside a clipping box is drawn through that box's chain: {drawing}"
    );
    let instances = drawing.instances();
    let ground = instances
        .iter()
        .position(|instance| instance.kind() == "quad")
        .expect("the root's background");
    let shape = instances
        .iter()
        .position(|instance| instance.kind() == "mono")
        .expect("the drawing");
    assert!(
        shape > ground,
        "the drawing draws over the root's background: {drawing}"
    );
}

/// Two outlines are two shapes with two identities, so a rasteriser can cache each on its own.
#[test]
fn each_outline_is_its_own_shape() {
    let tree = Element::new("root").children(vec![Element::new("mark").drawing(
        "M0 0 L24 0 L24 24 Z\nM0 0 L0 24 L24 24 Z",
        Some("0 0 24 24"),
    )]);
    let mut harness = Harness::new(tree, CSS);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);

    let recording = recorded(&harness, &cache);
    let items = recording.paths();
    assert_eq!(items.len(), 2);
    assert!(
        !std::sync::Arc::ptr_eq(&items[0].path, &items[1].path),
        "two marks, drawn twice"
    );
    assert_eq!(shapes(&harness.redraw()).len(), 2);
}

/// An element that carries no outlines produces no vector primitive, and does not become one by
/// having a `<vector>`-shaped box.
#[test]
fn an_element_with_no_outlines_draws_nothing() {
    let tree = Element::new("root").children(vec![Element::new("mark")]);
    let mut harness = Harness::new(tree, CSS);
    let cache = VectorCache::new();
    harness.paint_vectors(&cache);
    assert!(shapes(&harness.redraw()).is_empty());
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
    let drawing = harness.redraw();
    assert!(
        shapes(&drawing).is_empty(),
        "a drawing every colour of which is fully transparent was still recorded: {drawing}"
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
    assert!(shapes(&harness.redraw()).is_empty());
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
    let drawing = harness.redraw();
    let shapes = shapes(&drawing);
    assert_eq!(shapes.len(), 1, "the drawing is still there: {drawing}");
    assert!(
        near(color(shapes[0]), [0.0, 0.1255, 0.25, 0.25]),
        "the group's alpha is folded into the shape's own paint: {drawing}"
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
    let before = shapes(&harness.redraw())[0].text().to_owned();

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

    let after = shapes(&harness.redraw())[0].text().to_owned();
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
        .device_ink(&zgui_space::Placements::of(&harness.scene.spatial));
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
        .content_box();
    let bounds = recorded_bounds(&harness, &cache);
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
/// The frames after the first replay the drawing's chunk. A replay that dropped it while the
/// damage that reached the fragment had already cleared its pixels would leave a hole that stays
/// until something repaints the whole surface. This is the regression this test exists to catch.
#[test]
fn an_unchanged_drawing_is_drawn_again_on_every_frame_that_reaches_it() {
    let mut harness = Harness::new(tree(), CSS);
    let cache = VectorCache::new();

    for frame in 1..=4 {
        harness.paint_vectors(&cache);
        let drawing = harness.redraw();
        assert_eq!(
            shapes(&drawing).len(),
            1,
            "frame {frame} reached the drawing and drew nothing for it: {drawing}"
        );
    }
}

/// A drawing scaled to nothing records nothing.
#[test]
fn a_drawing_scaled_to_nothing_records_nothing() {
    let css = "root { display: block; width: 400px; height: 400px }
               mark { display: block; width: 48px; height: 48px; color: rgb(0, 128, 255);
                      transform: scale(0) }";
    let mut harness = Harness::new(tree(), css);
    let report = paint_cached(&mut harness);
    assert!(report.vector_routes[0].routes.is_empty());
    assert!(shapes(&harness.redraw()).is_empty());
}
