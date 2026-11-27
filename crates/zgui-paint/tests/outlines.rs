//! Outlines: drawn as the walk leaves a box, recoloured by an animation of that box only.

mod support;

use support::{Element, Harness};
use zgui_bits::DamageSet;
use zgui_css::values::color::AbsoluteColor;
use zgui_dom::NodeKey;
use zgui_dom::side::AnimOverride;
use zgui_geom::{Device, Point, Rect, Size};
use zgui_paint::AnimOverrides;
use zgui_testkit_compositor::Instance;

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

/// The outlines the compositor draws, which are the quads with a 2px border on every side here,
/// in draw order.
fn outlines(harness: &mut Harness) -> Vec<Instance> {
    harness
        .redraw()
        .quads()
        .into_iter()
        .filter(|quad| quad.field("border") == Some("[2, 2, 2, 2]"))
        .cloned()
        .collect()
}

/// The solid colour `quad` is stroked with.
fn stroke(quad: &Instance) -> [f32; 4] {
    quad.color("stroke")
        .unwrap_or_else(|| panic!("an outline stroked with one colour: {}", quad.text()))
}

fn blue() -> [f32; 4] {
    [0.0, 0.0, 1.0, 1.0]
}

fn red() -> [f32; 4] {
    [1.0, 0.0, 0.0, 1.0]
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
fn ring_colours(harness: &mut Harness) -> [[f32; 4]; 2] {
    let mut rings = outlines(harness);
    assert_eq!(rings.len(), 2, "one outline per ring");
    rings.sort_by(|one, other| one.bounds()[1].total_cmp(&other.bounds()[1]));
    [stroke(&rings[0]), stroke(&rings[1])]
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
    harness.paint_animated(&anim);
    assert_eq!(ring_colours(&mut harness), [red(), blue()]);

    // A second frame reuses the walk's containers and must draw the same thing.
    harness.paint_animated(&anim);
    assert_eq!(ring_colours(&mut harness), [red(), blue()]);

    // The style both rings share was never written: with the animation over, both are blue.
    harness.paint_animated(&zgui_paint::NoAnim);
    assert_eq!(ring_colours(&mut harness), [blue(), blue()]);
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
    let rings = outlines(&mut harness);
    assert_eq!(rings.len(), 2, "one outline per outlined box");
    let [inner, outer] = &rings[..] else {
        unreachable!("two rings");
    };
    assert_eq!(
        inner.bounds()[2],
        104.0,
        "the inner outline, 2px outside a 100px box, is drawn first"
    );
    assert_eq!(
        outer.bounds()[2],
        204.0,
        "the outer outline, 2px outside a 200px box"
    );
    assert_eq!(stroke(inner), red());
    assert_eq!(stroke(outer), blue());
}

#[test]
fn an_unchanged_outline_stays_drawn_and_is_recorded_once() {
    let mut harness = Harness::sized(
        Element::new("root").children(vec![Element::new("ring"), Element::new("far")]),
        "root { display: block; width: 300px }
         ring { display: block; height: 40px; outline: 2px solid blue }
         far { display: block; height: 40px; margin-top: 400px; background: #eee }",
        300.0,
        600.0,
    );
    harness.paint_everything();
    assert_eq!(outlines(&mut harness).len(), 1);

    for (y, near) in [(0, "over the ring"), (500, "far from the ring")] {
        harness.clear_damage();
        harness.damage.absorb(Rect::<i32, Device>::new(
            Point::new(0, y),
            Size::new(300, 20),
        ));
        harness.paint();
        assert_eq!(harness.compositor.sync.recorded, 0, "damage {near}");
        assert_eq!(outlines(&mut harness).len(), 1, "damage {near}");
    }
}
