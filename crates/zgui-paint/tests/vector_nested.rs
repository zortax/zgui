//! Drawings nested in cards, recorded with the backgrounds around them into one chunk.
//!
//! An interface never draws a lone drawing on a bare surface. A drawing sits inside a card inside
//! a row, with a second card beside it, and a badge may be painted over it after it. Each case runs
//! the whole chain, from document source on an element, read and fitted by the paint stage's own
//! vector source, recorded with the enclosing backgrounds and drawn by the compositor, and asserts
//! colours at coordinates.

mod support;

use zgui_color::Color;
use zgui_dom::{Document, NodeKind};
use zgui_geom::{DevicePx, Point, Rect, Size};
use zgui_interned::ElementName;
use zgui_paint::VectorId;
use zgui_paint::content::VectorPlacement as Placement;
use zgui_paint::emit::Recorder;
use zgui_paint::emit::vector::{ShapePaint, VectorPlacement};
use zgui_paint::{VectorCache, VectorSource};
use zgui_space::{ClipId, SpatialId};
use zgui_testkit_compositor::Pixels;
use zgui_vocab::{PropKey, PropValue, prop::drawing};

use support::conformance::scene_at;
use support::gpu::Gpu;

/// A document filled with `colour` over its whole view box.
fn filled(colour: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
             <rect x="0" y="0" width="24" height="24" fill="{colour}"/>
           </svg>"##
    )
}

/// Records the document `source` fitted to `box_` as the shapes of drawing `id`.
fn draw_document(rec: &mut Recorder<'_>, id: u32, source: &str, box_: (f32, f32, f32, f32)) {
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
        rec,
        VectorId(id),
        &drawing.placed_all(),
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

/// Records an opaque quad of `[red, green, blue]` over `(x, y, width, height)`.
fn quad(rec: &mut Recorder<'_>, (x, y, width, height): (f32, f32, f32, f32), rgb: [u8; 3]) {
    let paint = rec.color(Color::srgb_u8(rgb[0], rgb[1], rgb[2], 255));
    let quad = zgui_paint::emit::record::filled(
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        ),
        [0.0; 8],
        paint,
    );
    rec.quad(quad);
}

/// The colour at `(x, y)`.
fn at(pixels: &Pixels, x: i32, y: i32) -> [u8; 3] {
    let [red, green, blue, _] = pixels.rgba(x, y);
    [red, green, blue]
}

/// Reports whether `found` is `wanted`, within four levels a channel.
fn is(found: [u8; 3], wanted: [u8; 3]) -> bool {
    (0..3)
        .map(|channel| (i32::from(found[channel]) - i32::from(wanted[channel])).abs())
        .max()
        .unwrap_or(0)
        <= 4
}

/// The deeply nested card: three backgrounds, then a drawing inside them all.
fn deep_card(rec: &mut Recorder<'_>, id: u32) {
    for (step, inset) in [0.0f32, 4.0, 8.0].into_iter().enumerate() {
        let level = 24 + step as u8 * 8;
        quad(
            rec,
            (
                8.0 + inset,
                8.0 + inset,
                48.0 - inset * 2.0,
                48.0 - inset * 2.0,
            ),
            [level, level, level],
        );
    }
    draw_document(rec, id, &filled("#e01020"), (20.0, 20.0, 24.0, 24.0));
}

/// The shallow card: one background, then a drawing inside it.
fn shallow_card(rec: &mut Recorder<'_>, id: u32) {
    quad(rec, (72.0, 8.0, 48.0, 48.0), [24, 24, 24]);
    draw_document(rec, id, &filled("#10c040"), (80.0, 20.0, 24.0, 24.0));
}

/// Draws a page holding both cards, the deep one first when `deep_first` is true, and then what
/// `after` records.
fn page(gpu: &mut Gpu, deep_first: bool, after: impl FnOnce(&mut Recorder<'_>)) -> Pixels {
    let mut scene = scene_at(gpu.side());
    let picture = gpu.picture(&mut scene, SpatialId::VIEWPORT, |rec| {
        quad(rec, (0.0, 0.0, 128.0, 128.0), [8, 10, 18]);
        if deep_first {
            deep_card(rec, 1);
            shallow_card(rec, 2);
        } else {
            shallow_card(rec, 2);
            deep_card(rec, 1);
        }
        after(rec);
    });
    std::sync::Arc::unwrap_or_clone(picture.pixels)
}

#[test]
fn a_drawing_nested_deeper_than_its_neighbour_is_not_erased_by_its_own_backgrounds() {
    let Some(mut gpu) = Gpu::open(128) else {
        return;
    };
    let pixels = page(&mut gpu, true, |_| {});
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

/// A badge painted over a drawing stays over it, whichever card was recorded first.
#[test]
fn a_primitive_painted_over_a_drawing_stays_over_it() {
    let Some(mut gpu) = Gpu::open(128) else {
        return;
    };
    let mut wrong = Vec::new();
    for deep_first in [true, false] {
        let pixels = page(&mut gpu, deep_first, |rec| {
            quad(rec, (84.0, 24.0, 12.0, 12.0), [0, 80, 255]);
        });
        let (badge, deep) = (at(&pixels, 90, 30), at(&pixels, 32, 32));
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
