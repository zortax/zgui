//! Drawings reaching the display list through the emit walk.
//!
//! Every assertion here goes through [`Painter::emit`](zgui_paint::Painter::emit) over a real box
//! tree, a real layout pass and real fragments, and reads the result out of a renderer rather than
//! out of the scene the test filled. That is the whole point: a drawing that only reaches the
//! display list when a test calls the emitter itself is a drawing an application cannot draw, and
//! an assertion made by calling the emitter is an assertion that cannot tell the difference.

mod support;

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
    let shapes = shapes(harness.scene());
    assert_eq!(shapes.len(), 1, "one outline is one shape: {shapes:?}");
    assert!(
        shapes[0].contains("fill=solid srgb(0, 0.502, 1, 1)"),
        "the fill is the element's own computed colour: {}",
        shapes[0]
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
/// This is the one thing the replay cache cannot do for a drawing. An unchanged fragment replays
/// the range of the operation log its primitives occupied last frame — and a drawing's primitives
/// are not in that log. A vector item is planned into a rasterisation pass instead, out of a list
/// the scene rebuilds every frame, so a replayed range re-emits nothing at all for it while the
/// damage that reached the fragment has already cleared its pixels. The icon disappears, and
/// because the following frames no longer damage the hole it stays gone until something repaints
/// the whole surface.
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
