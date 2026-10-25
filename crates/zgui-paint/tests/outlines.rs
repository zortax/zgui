//! Outlines: drawn as the walk leaves a box, recoloured by an animation of that box only.

mod support;

use support::{Element, Harness};
use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_css::values::color::AbsoluteColor;
use zgui_dom::NodeKey;
use zgui_dom::side::AnimOverride;
use zgui_geom::{Device, Point, Rect, Size};
use zgui_paint::{AnimOverrides, PaintInput, PaintReport};
use zgui_scene::{Paint, Quad};

/// One element animating its outline colour.
struct OneAnimating {
    node: NodeKey,
    over: AnimOverride,
}

impl AnimOverrides for OneAnimating {
    fn get(&self, node: NodeKey) -> Option<&AnimOverride> {
        (node == self.node).then_some(&self.over)
    }
}

/// Emits a frame against the harness damage with `anim` driving the overrides.
fn paint_with(harness: &mut Harness, anim: &dyn AnimOverrides) -> PaintReport {
    harness.scene.begin_frame(harness.viewport);
    let mut input = PaintInput::new(&harness.store, &harness.damage);
    input.anim = anim;
    let report = harness.painter.emit(&input, &mut harness.scene);
    harness.scene.finish(&harness.damage);
    report
}

/// The quads of this frame with a 2px border on every side, which only an outline draws here.
fn outlines(harness: &Harness) -> Vec<Quad> {
    harness
        .scene
        .primitives
        .quads
        .iter()
        .filter(|quad| quad.border == [2.0; 4])
        .copied()
        .collect()
}

/// The solid colour `quad` is stroked with.
fn stroke(harness: &Harness, quad: &Quad) -> Color {
    match harness
        .scene
        .paints
        .get(quad.stroke.id().expect("a stroke"))
    {
        Some(Paint::Solid(color)) => *color,
        other => panic!("an outline stroked with {other:?}"),
    }
}

fn blue() -> Color {
    Color::srgb(0.0, 0.0, 1.0, 1.0)
}

fn red() -> Color {
    Color::srgb(1.0, 0.0, 0.0, 1.0)
}

/// Two boxes of one class, which share one lowered style.
fn two_rings() -> Harness {
    Harness::new(
        Element::new("root").children(vec![
            Element::new("a").classes(&["ring"]),
            Element::new("b").classes(&["ring"]),
        ]),
        "root { display: block; width: 300px }
         .ring { display: block; height: 40px; margin: 10px; outline: 2px solid blue }",
    )
}

/// The outline colours of `a` and `b`, in that order.
fn ring_colours(harness: &Harness) -> [Color; 2] {
    let mut rings = outlines(harness);
    assert_eq!(rings.len(), 2, "one outline per ring");
    rings.sort_by(|one, other| one.bounds[1].total_cmp(&other.bounds[1]));
    [stroke(harness, &rings[0]), stroke(harness, &rings[1])]
}

#[test]
fn an_animated_outline_colour_recolours_only_its_own_box() {
    let mut harness = two_rings();
    let anim = OneAnimating {
        node: harness.document.store().key_of(harness.element("a")),
        over: AnimOverride {
            outline_color: Some(AbsoluteColor::srgb_legacy(255, 0, 0, 1.0)),
            ..AnimOverride::new()
        },
    };
    harness.damage = DamageSet::full();
    paint_with(&mut harness, &anim);
    assert_eq!(ring_colours(&harness), [red(), blue()]);

    // A second frame reuses the walk's containers and must draw the same thing.
    paint_with(&mut harness, &anim);
    assert_eq!(ring_colours(&harness), [red(), blue()]);

    // The style both rings share was never written: with the animation over, both are blue.
    paint_with(&mut harness, &zgui_paint::NoAnim);
    assert_eq!(ring_colours(&harness), [blue(), blue()]);
}

#[test]
fn each_outline_belongs_to_the_box_that_declared_it() {
    // An outlined box inside an un-outlined one inside an outlined one: the outline stack pushes
    // an entry for every entered box, so each leave pops its own.
    let mut harness = Harness::new(
        Element::new("root").children(vec![Element::new("outer").children(vec![
            Element::new("middle").children(vec![Element::new("inner")]),
        ])]),
        "root { display: block; width: 300px }
         outer { display: block; width: 200px; outline: 2px solid blue }
         middle { display: block; width: 150px }
         inner { display: block; width: 100px; height: 20px; outline: 2px solid red }",
    );
    harness.paint_everything();
    let rings = outlines(&harness);
    assert_eq!(rings.len(), 2, "one outline per outlined box");
    let inner = rings
        .iter()
        .find(|quad| quad.bounds[2] == 104.0)
        .expect("the inner outline, 2px outside a 100px box");
    let outer = rings
        .iter()
        .find(|quad| quad.bounds[2] == 204.0)
        .expect("the outer outline, 2px outside a 200px box");
    assert_eq!(stroke(&harness, inner), red());
    assert_eq!(stroke(&harness, outer), blue());
    assert!(
        outer.order > inner.order,
        "the outer outline at {} sorts under the inner one at {}",
        outer.order,
        inner.order
    );
}

#[test]
fn an_outline_is_drawn_only_on_frames_whose_damage_reaches_it() {
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("ring"), Element::new("far")]),
        "root { display: block; width: 300px }
         ring { display: block; height: 40px; outline: 2px solid blue }
         far { display: block; height: 40px; margin-top: 400px; background: #eee }",
        300.0,
        600.0,
    );
    harness.paint_everything();
    assert_eq!(outlines(&harness).len(), 1);

    harness.clear_damage();
    harness.damage.absorb(Rect::<i32, Device>::new(
        Point::new(0, 0),
        Size::new(300, 20),
    ));
    harness.paint();
    assert_eq!(outlines(&harness).len(), 1, "damage over the ring");

    harness.clear_damage();
    harness.damage.absorb(Rect::<i32, Device>::new(
        Point::new(0, 500),
        Size::new(300, 20),
    ));
    harness.paint();
    assert!(outlines(&harness).is_empty(), "damage far from the ring");
}
