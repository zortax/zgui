//! Two drawings on one surface, one of them nested deeper than the other.
//!
//! The arrangement every earlier test here was missing, and the one an interface always has: a
//! drawing does not sit on the bare surface, it sits inside a card inside a row inside a page, and
//! there is a second card beside it. Draw order is allocated from what a primitive overlaps rather
//! than counted up as a frame is emitted, so each card restarts from just above the page and the
//! more deeply nested drawing takes the *higher* order however early it was emitted. One
//! rasterisation pass covering both is composited by one draw, so a composite placed at the order of
//! the last item admitted lands underneath the backgrounds the other drawing is nested inside, and
//! those backgrounds erase it.
//!
//! A test with one drawing on an empty surface cannot see any of that. Nor can one whose drawings
//! sit at the same depth. So this is the whole chain — document source on an element, read and
//! fitted by the paint stage's own vector source, emitted by the real emitter into a display list
//! that also holds the enclosing backgrounds, planned, rasterised and composited — asserted as
//! colours at coordinates on the presented surface.

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

/// A document filling its own view box with one colour of its own.
fn filled(colour: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
             <rect x="0" y="0" width="24" height="24" fill="{colour}"/>
           </svg>"##
    )
}

/// Emits one document into `scene`, fitted to the given content box.
///
/// The identity is the caller's so that two drawings never share one, exactly as the paint walk
/// derives one per fragment.
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
            scale: 1.0,
        },
    );
}

/// The colour at a point, ignoring alpha.
fn at(pixels: &Pixels, x: i32, y: i32) -> [u8; 3] {
    let [red, green, blue, _] = pixels.rgba(x, y);
    [red, green, blue]
}

/// Whether a colour is what was asked for, within the rounding a device does.
fn is(found: [u8; 3], wanted: [u8; 3]) -> bool {
    (0..3)
        .map(|channel| (i32::from(found[channel]) - i32::from(wanted[channel])).abs())
        .max()
        .unwrap_or(0)
        <= 4
}

/// A page, a card three backgrounds deep holding one drawing, and a card one background deep
/// holding another.
fn two_cards() -> Scene {
    let mut scene = scene();
    quad(&mut scene, rect(0.0, 0.0, 128.0, 128.0), opaque(8, 10, 18));

    // The deeply nested card. Each background overlaps the one outside it, so each takes an order
    // above it, and the drawing inside them all takes the highest of the four.
    for (step, inset) in [0.0f32, 4.0, 8.0].into_iter().enumerate() {
        let level = 24 + step as u8 * 8;
        quad(
            &mut scene,
            rect(
                8.0 + inset,
                8.0 + inset,
                48.0 - inset * 2.0,
                48.0 - inset * 2.0,
            ),
            opaque(level, level, level),
        );
    }
    draw_document(&mut scene, 1, &filled("#e01020"), (20.0, 20.0, 24.0, 24.0));

    // The shallow card beside it, whose own drawing therefore takes a *lower* order than the first
    // card's — and is emitted last, which is what makes the two disagree.
    quad(&mut scene, rect(72.0, 8.0, 48.0, 48.0), opaque(24, 24, 24));
    draw_document(&mut scene, 2, &filled("#10c040"), (80.0, 20.0, 24.0, 24.0));

    scene
}

/// Both drawings are on the surface, each above the backgrounds it is nested inside.
#[test]
fn a_drawing_nested_deeper_than_its_neighbour_is_not_erased_by_its_own_backgrounds() {
    let Some(mut harness): Option<Harness> = harness(Which::Vello) else {
        eprintln!("skipped: no usable graphics device");
        return;
    };
    let mut scene = two_cards();
    scene.finish(&DamageSet::full());

    // The arrangement is only worth testing while the two really do share one pass composited by
    // one draw; two passes would make the ordering question disappear.
    assert_eq!(
        scene.pass_plan().len(),
        1,
        "nothing painted between the two drawings meets either one's ink, so they share a pass"
    );

    let pixels = support::present(&mut harness.renderer, &scene);
    assert!(
        is(at(&pixels, 32, 32), [0xe0, 0x10, 0x20]),
        "the drawing inside the nested card is buried under its own backgrounds: found {:?}",
        at(&pixels, 32, 32)
    );
    assert!(
        is(at(&pixels, 92, 32), [0x10, 0xc0, 0x40]),
        "the drawing inside the shallow card is missing: found {:?}",
        at(&pixels, 92, 32)
    );
}
