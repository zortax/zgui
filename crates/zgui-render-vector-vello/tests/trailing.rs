//! A probe: a primitive painted over a drawing *after* the pass's last item was admitted.
//!
//! Rule 3 is consulted only when a new vector item arrives, so a primitive emitted after a pass's
//! final item is recorded and never tested. If the pass composites above every item of it, such a
//! primitive can end up below the composite while the drawing it was painted over is inside it.

mod support;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_dom::{Document, NodeKind};
use zgui_geom::{DevicePx, Point, Rect, Size};
use zgui_interned::ElementName;
use zgui_paint::content::VectorPlacement as Placement;
use zgui_paint::emit::vector::{ShapePaint, VectorPlacement};
use zgui_paint::{VectorCache, VectorSource};
use zgui_render_wgpu::Pixels;
use zgui_scene::{ClipId, Scene, SpatialId, VectorId};
use zgui_vocab::{PropKey, PropValue, prop::drawing};

use support::{Harness, Which, harness, opaque, quad, rect, scene};

fn filled(colour: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
             <rect x="0" y="0" width="24" height="24" fill="{colour}"/>
           </svg>"##
    )
}

fn draw_document(scene: &mut Scene, id: u32, source: &str, box_: (f32, f32, f32, f32)) {
    let mut document = Document::new();
    let index = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("vector"),
    );
    document
        .edit(&zgui_dom::EverythingMatters, |edit| {
            edit.set_property(
                index,
                PropKey::new(drawing::DOCUMENT),
                Some(PropValue::from(source)),
            );
        })
        .expect("not poisoned");
    let node = document.store().key_of(index);

    let (left, top, width, height) = box_;
    let content_box = Rect::new(
        Point::new(DevicePx(left), DevicePx(top)),
        Size::new(DevicePx(width), DevicePx(height)),
    );
    let cache = VectorCache::new();
    let drawing = cache
        .frame(&document)
        .drawing(
            node,
            Placement {
                content_box,
                scale: 1.0,
            },
        )
        .expect("the element draws a document");

    zgui_paint::emit::vector::draw(
        scene,
        VectorId(id),
        &drawing.shapes,
        ShapePaint {
            fill: Color::srgb(0.0, 0.0, 0.0, 1.0),
            stroke: None,
            stroke_width: 1.0,
        },
        VectorPlacement {
            clip: ClipId::ROOT,
            transform: SpatialId::VIEWPORT,
        },
    );
}

fn at(pixels: &Pixels, x: i32, y: i32) -> [u8; 3] {
    let [red, green, blue, _] = pixels.rgba(x, y);
    [red, green, blue]
}

fn is(found: [u8; 3], wanted: [u8; 3]) -> bool {
    (0..3)
        .map(|channel| (i32::from(found[channel]) - i32::from(wanted[channel])).abs())
        .max()
        .unwrap_or(0)
        <= 4
}

/// The deeply nested card: three backgrounds, then a drawing inside them all.
fn deep_card(scene: &mut Scene, id: u32) {
    for (step, inset) in [0.0f32, 4.0, 8.0].into_iter().enumerate() {
        let level = 24 + step as u8 * 8;
        quad(
            scene,
            rect(
                8.0 + inset,
                8.0 + inset,
                48.0 - inset * 2.0,
                48.0 - inset * 2.0,
            ),
            opaque(level, level, level),
        );
    }
    draw_document(scene, id, &filled("#e01020"), (20.0, 20.0, 24.0, 24.0));
}

/// The shallow card: one background, then a drawing inside it.
fn shallow_card(scene: &mut Scene, id: u32) {
    quad(scene, rect(72.0, 8.0, 48.0, 48.0), opaque(24, 24, 24));
    draw_document(scene, id, &filled("#10c040"), (80.0, 20.0, 24.0, 24.0));
}

/// Both cards and then a badge over the shallow card's drawing, with the cards in a chosen order.
fn badge_over_the_shallow_drawing(deep_first: bool) -> Scene {
    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(8, 10, 18));

    if deep_first {
        deep_card(&mut scene, 1);
        shallow_card(&mut scene, 2);
    } else {
        shallow_card(&mut scene, 2);
        deep_card(&mut scene, 1);
    }

    // Painted after both drawings, over the shallow card's drawing.
    quad(&mut scene, rect(84.0, 24.0, 12.0, 12.0), opaque(0, 80, 255));

    scene
}

/// A badge painted over a drawing stays over it, whichever card was emitted first.
#[test]
fn a_primitive_painted_over_a_drawing_of_a_pass_is_not_swallowed_by_the_composite() {
    let Some(mut harness): Option<Harness> = harness(Which::Vello) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let mut wrong = Vec::new();
    for deep_first in [true, false] {
        let mut scene = badge_over_the_shallow_drawing(deep_first);
        scene.finish(&DamageSet::full());
        let passes = scene.pass_plan().len();

        let pixels = support::present(&mut harness.renderer, &scene);
        let (badge, deep) = (at(&pixels, 90, 30), at(&pixels, 32, 32));
        eprintln!("deep_first={deep_first} passes={passes} badge={badge:?} deep={deep:?}");
        if !is(badge, [0x00, 0x50, 0xff]) {
            wrong.push(format!(
                "deep_first={deep_first}: the badge painted over the drawing is underneath it \
                 ({badge:?})"
            ));
        }
        if !is(deep, [0xe0, 0x10, 0x20]) {
            wrong.push(format!(
                "deep_first={deep_first}: the deep drawing is buried ({deep:?})"
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("; "));
}
