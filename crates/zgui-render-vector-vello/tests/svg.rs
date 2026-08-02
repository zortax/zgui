//! What a vector document actually draws, on a real device.
//!
//! Every case here runs the whole chain: the source text is set on an element of a real document,
//! read by the real paint-stage vector source, fitted to a real content box, emitted by the real
//! emitter, rasterised by this crate and read back as pixels. Nothing here asserts that a primitive
//! was emitted. A display list full of vector items composites a scratch nobody wrote just as
//! happily as one that draws, and a document whose colours were resolved wrongly produces exactly
//! as many items as one whose colours were resolved rightly — so the assertions are about colours
//! at coordinates.

mod support;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_dom::{Document, NodeKind};
use zgui_geom::{DevicePx, Point, Rect, Scale, Size};
use zgui_interned::ElementName;
use zgui_paint::content::VectorPlacement as Placement;
use zgui_paint::emit::vector::{ShapePaint, VectorPlacement};
use zgui_paint::{VectorCache, VectorSource};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Builder, Pixels, WgpuRenderer, wgpu};
use zgui_scene::{ClipId, Scene, SpatialId, VectorId};
use zgui_vocab::{PropKey, PropValue, prop::drawing};

use support::{Harness, SIDE, Which, difference, harness_at, present, twins, whole_pixels};

/// The extent every case here draws into.
const EXTENT: i32 = 128;

/// A renderer at this file's extent with the path renderer attached.
fn renderer() -> Option<Harness> {
    let _ = SIDE;
    harness_at(EXTENT, Which::Vello)
}

/// Where a document is drawn, and what colour it inherits there.
#[derive(Clone, Copy)]
struct Drawn {
    /// The content box, as left, top, width and height in device pixels.
    box_: (f32, f32, f32, f32),
    /// The colour the element resolves `currentColor` to.
    color: Color,
}

impl Drawn {
    /// The whole surface, with black inherited.
    fn whole() -> Self {
        Self {
            box_: (0.0, 0.0, EXTENT as f32, EXTENT as f32),
            color: Color::srgb(0.0, 0.0, 0.0, 1.0),
        }
    }

    /// The same, inheriting `color`.
    fn inheriting(self, color: Color) -> Self {
        Self { color, ..self }
    }

    /// The same, drawn into the given box.
    fn into_box(self, left: f32, top: f32, width: f32, height: f32) -> Self {
        Self {
            box_: (left, top, width, height),
            ..self
        }
    }
}

/// Builds the display list one document is drawn through.
///
/// The stages are the real ones and are wired in the real order: the source is a property of an
/// element in a real document, the paint stage's own vector source reads and fits it, and the
/// emitter turns the shapes into primitives.
fn scene_of(source: &str, drawn: Drawn) -> Scene {
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

    let (left, top, width, height) = drawn.box_;
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

    let mut scene = Scene::new();
    scene.begin_frame(Size::new(EXTENT, EXTENT));
    zgui_paint::emit::vector::draw(
        &mut scene,
        VectorId(1),
        &drawing.shapes,
        ShapePaint {
            fill: drawn.color,
            stroke: None,
            stroke_width: 1.0,
        },
        VectorPlacement {
            clip: ClipId::ROOT,
            transform: SpatialId::VIEWPORT,
        },
    );
    scene.finish(&DamageSet::full());
    scene
}

/// Draws one document and reads the surface back.
fn drawn(harness: &mut Harness, source: &str, how: Drawn) -> Pixels {
    let scene = scene_of(source, how);
    present(harness, &scene)
}

/// The colour at a point, ignoring alpha.
fn at(pixels: &Pixels, x: i32, y: i32) -> [u8; 3] {
    let [red, green, blue, _] = pixels.rgba(x, y);
    [red, green, blue]
}

/// Whether a pixel has anything drawn on it.
fn painted(pixels: &Pixels, x: i32, y: i32) -> bool {
    pixels.rgba(x, y)[3] > 32
}

/// How far apart two colours are, on the widest channel.
fn apart(one: [u8; 3], two: [u8; 3]) -> i32 {
    (0..3)
        .map(|channel| (i32::from(one[channel]) - i32::from(two[channel])).abs())
        .max()
        .unwrap_or(0)
}

/// Whether a colour is what was asked for, within the rounding a device does.
fn is(found: [u8; 3], wanted: [u8; 3]) -> bool {
    apart(found, wanted) <= 4
}

/// The smallest rectangle containing every painted pixel, as left, top, right, bottom.
fn ink_extent(pixels: &Pixels) -> Option<(i32, i32, i32, i32)> {
    let mut found: Option<(i32, i32, i32, i32)> = None;
    for y in 0..EXTENT {
        for x in 0..EXTENT {
            if !painted(pixels, x, y) {
                continue;
            }
            found = Some(match found {
                None => (x, y, x, y),
                Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x), b.max(y)),
            });
        }
    }
    found
}

/// A document whose two halves are two different colours of its own.
const TWO_COLOURS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
  <rect x="0" y="0" width="64" height="128" fill="#e01020"/>
  <rect x="64" y="0" width="64" height="128" fill="#1030d0"/>
</svg>"##;

/// The same shape twice, once asking for the inherited colour.
const CURRENT_COLOUR: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
  <rect x="0" y="0" width="128" height="128" fill="currentColor"/>
</svg>"##;

#[test]
fn a_documents_own_colours_arrive_at_the_places_the_document_put_them() {
    let Some(mut harness) = renderer() else {
        return;
    };
    let pixels = drawn(&mut harness, TWO_COLOURS, Drawn::whole());
    assert!(
        is(at(&pixels, 32, 64), [0xe0, 0x10, 0x20]),
        "the left half is the first fill, not {:?}",
        at(&pixels, 32, 64)
    );
    assert!(
        is(at(&pixels, 96, 64), [0x10, 0x30, 0xd0]),
        "the right half is the second fill, not {:?}",
        at(&pixels, 96, 64)
    );
}

/// The rule the whole colour scheme exists for: a document with its own fills keeps every one of
/// them, whatever colour the element around it is.
#[test]
fn a_document_with_its_own_colours_is_not_tinted_by_the_element() {
    let Some(mut harness) = renderer() else {
        return;
    };
    let green = drawn(
        &mut harness,
        TWO_COLOURS,
        Drawn::whole().inheriting(Color::srgb(0.0, 1.0, 0.0, 1.0)),
    );
    let magenta = drawn(
        &mut harness,
        TWO_COLOURS,
        Drawn::whole().inheriting(Color::srgb(1.0, 0.0, 1.0, 1.0)),
    );
    for (x, y) in [(32, 64), (96, 64), (10, 10), (120, 120)] {
        assert_eq!(
            at(&green, x, y),
            at(&magenta, x, y),
            "at ({x}, {y}) the document's own colour moved when the element's did"
        );
    }
    assert!(is(at(&green, 32, 64), [0xe0, 0x10, 0x20]));
}

/// And its opposite: a document that asked for the inherited colour gets it, and follows it.
#[test]
fn a_current_colour_document_takes_the_elements_colour_and_moves_with_it() {
    let Some(mut harness) = renderer() else {
        return;
    };
    let green = drawn(
        &mut harness,
        CURRENT_COLOUR,
        Drawn::whole().inheriting(Color::srgb(0.0, 0.8, 0.2, 1.0)),
    );
    assert!(
        is(at(&green, 64, 64), [0x00, 0xcc, 0x33]),
        "an inherited fill is the element's colour, not {:?}",
        at(&green, 64, 64)
    );

    let orange = drawn(
        &mut harness,
        CURRENT_COLOUR,
        Drawn::whole().inheriting(Color::srgb(1.0, 0.5, 0.0, 1.0)),
    );
    assert!(
        is(at(&orange, 64, 64), [0xff, 0x80, 0x00]),
        "changing the element's colour re-colours the document, but it stayed {:?}",
        at(&orange, 64, 64)
    );
}

/// A gradient has to be a ramp on the surface, not the average of its stops.
#[test]
fn a_gradient_is_drawn_as_a_ramp_rather_than_as_a_flat_fill() {
    let Some(mut harness) = renderer() else {
        return;
    };
    const RAMP: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
      <defs>
        <linearGradient id="g" x1="0" y1="0" x2="128" y2="0" gradientUnits="userSpaceOnUse">
          <stop offset="0" stop-color="#ff0000"/>
          <stop offset="1" stop-color="#0000ff"/>
        </linearGradient>
      </defs>
      <rect x="0" y="0" width="128" height="128" fill="url(#g)"/>
    </svg>"##;
    let pixels = drawn(&mut harness, RAMP, Drawn::whole());

    let samples: Vec<[u8; 3]> = [8, 32, 64, 96, 120]
        .into_iter()
        .map(|x| at(&pixels, x, 64))
        .collect();
    assert!(
        samples.windows(2).all(|pair| pair[1][0] < pair[0][0]),
        "red has to fall across the ramp: {samples:?}"
    );
    assert!(
        samples.windows(2).all(|pair| pair[1][2] > pair[0][2]),
        "blue has to rise across the ramp: {samples:?}"
    );
    assert!(
        is(samples[0], [0xf8, 0x00, 0x08]) || apart(samples[0], [0xff, 0x00, 0x00]) < 24,
        "the ramp starts at its first stop, not {:?}",
        samples[0]
    );
    assert!(
        apart(samples[4], [0x00, 0x00, 0xff]) < 24,
        "and ends at its last, not {:?}",
        samples[4]
    );
}

/// A radial ramp with a `reflect` spread comes back to its first colour at twice the radius.
///
/// There is no reflected spread in the model this framework draws through — a reflected ramp is
/// expressed as a repeating one of twice the extent with a mirrored ramp — so this is the case that
/// says the expression is the same picture and not merely the same number of stops.
#[test]
fn a_reflected_ramp_returns_to_its_first_colour_at_twice_the_distance() {
    let Some(mut harness) = renderer() else {
        return;
    };
    const REFLECTED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
      <defs>
        <linearGradient id="g" x1="0" y1="0" x2="32" y2="0" spreadMethod="reflect"
                        gradientUnits="userSpaceOnUse">
          <stop offset="0" stop-color="#ff0000"/>
          <stop offset="1" stop-color="#00ff00"/>
        </linearGradient>
      </defs>
      <rect x="0" y="0" width="128" height="128" fill="url(#g)"/>
    </svg>"##;
    let pixels = drawn(&mut harness, REFLECTED, Drawn::whole());
    // Red at nothing, green a quarter along, red again at half — which a padded or a repeating
    // ramp would both get wrong, the first by holding green and the second by jumping back to red.
    assert!(
        apart(at(&pixels, 1, 64), [0xff, 0x00, 0x00]) < 24,
        "{:?}",
        at(&pixels, 1, 64)
    );
    assert!(
        apart(at(&pixels, 31, 64), [0x00, 0xff, 0x00]) < 24,
        "{:?}",
        at(&pixels, 31, 64)
    );
    assert!(
        apart(at(&pixels, 63, 64), [0xff, 0x00, 0x00]) < 24,
        "{:?}",
        at(&pixels, 63, 64)
    );
    assert!(
        apart(at(&pixels, 95, 64), [0x00, 0xff, 0x00]) < 24,
        "{:?}",
        at(&pixels, 95, 64)
    );
}

/// A clip has to remove pixels, and the same document without it has to keep them.
#[test]
fn a_clipped_group_draws_only_inside_its_clip() {
    let Some(mut harness) = renderer() else {
        return;
    };
    const CLIPPED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
      <defs>
        <clipPath id="c"><rect x="32" y="32" width="64" height="64"/></clipPath>
      </defs>
      <g clip-path="url(#c)">
        <rect x="0" y="0" width="128" height="128" fill="#20a040"/>
      </g>
    </svg>"##;
    const UNCLIPPED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
      <g>
        <rect x="0" y="0" width="128" height="128" fill="#20a040"/>
      </g>
    </svg>"##;

    let open = drawn(&mut harness, UNCLIPPED, Drawn::whole());
    assert!(
        painted(&open, 8, 8) && painted(&open, 64, 64),
        "the control has to cover the whole box, or the clip proves nothing"
    );

    let clipped = drawn(&mut harness, CLIPPED, Drawn::whole());
    assert!(
        painted(&clipped, 64, 64),
        "inside the clip the fill survives"
    );
    for (x, y) in [(8, 8), (120, 8), (8, 120), (120, 120), (16, 64), (64, 16)] {
        assert!(
            !painted(&clipped, x, y),
            "({x}, {y}) is outside the clip and must be blank"
        );
    }
    assert_eq!(
        ink_extent(&clipped),
        Some((32, 32, 95, 95)),
        "the ink is exactly the clip rectangle"
    );
}

/// A clip written even-odd is a ring, and a clip written non-zero over the same outlines is a disc.
#[test]
fn a_clip_rule_decides_what_the_clip_keeps() {
    let Some(mut harness) = renderer() else {
        return;
    };
    let document = |rule: &str| {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
      <defs>
        <clipPath id="c">
          <path clip-rule="{rule}"
                d="M8 8 H120 V120 H8 Z M48 48 H80 V80 H48 Z"/>
        </clipPath>
      </defs>
      <g clip-path="url(#c)">
        <rect x="0" y="0" width="128" height="128" fill="#20a040"/>
      </g>
    </svg>"##
        )
    };
    let hollow = drawn(&mut harness, &document("evenodd"), Drawn::whole());
    let solid = drawn(&mut harness, &document("nonzero"), Drawn::whole());
    assert!(
        painted(&hollow, 20, 20) && painted(&solid, 20, 20),
        "both rules keep the outer band"
    );
    assert!(
        !painted(&hollow, 64, 64),
        "the even-odd rule punches the inner square out"
    );
    assert!(
        painted(&solid, 64, 64),
        "and the non-zero rule does not, or the rule was never read"
    );
}

/// A stroke is more than its width: caps, joins and dashes are the outline it stands for.
#[test]
fn a_dashed_stroke_leaves_the_gaps_it_asked_for() {
    let Some(mut harness) = renderer() else {
        return;
    };
    let document = |dashes: &str| {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
      <path d="M0 64 H128" stroke="#000000" stroke-width="24" {dashes}/>
    </svg>"##
        )
    };
    let solid = drawn(&mut harness, &document(""), Drawn::whole());
    let dashed = drawn(
        &mut harness,
        &document(r#"stroke-dasharray="16 16""#),
        Drawn::whole(),
    );
    let along = |pixels: &Pixels| (0..EXTENT).filter(|x| painted(pixels, *x, 64)).count();
    assert_eq!(along(&solid), EXTENT as usize, "a solid stroke has no gaps");
    let covered = along(&dashed);
    assert!(
        (56..=72).contains(&covered),
        "a sixteen-on sixteen-off pattern covers about half the line, not {covered} of {EXTENT}"
    );
}

/// A drawing has to fit its box the way its space says, at every shape of box.
#[test]
fn a_view_box_is_fitted_uniformly_and_centred_at_every_box_shape() {
    let Some(mut harness) = renderer() else {
        return;
    };
    // A twenty by ten space filled edge to edge: whatever it is fitted into, the ink is the
    // rectangle the fit produces and nothing else.
    const WIDE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 10">
      <rect x="0" y="0" width="20" height="10" fill="#000000"/>
    </svg>"##;

    // A square box: the drawing is as wide as the box and half as tall, centred vertically.
    let square = drawn(
        &mut harness,
        WIDE,
        Drawn::whole().into_box(0.0, 0.0, 128.0, 128.0),
    );
    assert_eq!(ink_extent(&square), Some((0, 32, 127, 95)));

    // A box of the drawing's own shape: it fills it exactly.
    let matching = drawn(
        &mut harness,
        WIDE,
        Drawn::whole().into_box(0.0, 0.0, 128.0, 64.0),
    );
    assert_eq!(ink_extent(&matching), Some((0, 0, 127, 63)));

    // A tall box: the width limits it, and the slack goes above and below in equal shares.
    let tall = drawn(
        &mut harness,
        WIDE,
        Drawn::whole().into_box(24.0, 0.0, 64.0, 128.0),
    );
    assert_eq!(ink_extent(&tall), Some((24, 48, 87, 79)));
}

/// A document's own `preserveAspectRatio` decides how its `viewBox` maps onto its own size.
///
/// This is the half of the fitting the parser does rather than the framework, and it is a
/// different question from which box the drawing is placed in: `none` means the drawing stretches
/// inside its own extent, which then goes into the element's box like any other.
#[test]
fn a_documents_own_aspect_ratio_reaches_the_picture() {
    let Some(mut harness) = renderer() else {
        return;
    };
    // A square drawn in a square space, in a document twice as wide as it is tall.
    let document = |aspect: &str| {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32" viewBox="0 0 32 32"
                preserveAspectRatio="{aspect}">
      <rect x="0" y="0" width="32" height="32" fill="#000000"/>
    </svg>"##
        )
    };
    // `meet` keeps it square and centres it in the document's own wider extent; the whole
    // document then fits the surface, so the ink is a centred square.
    let met = drawn(&mut harness, &document("xMidYMid meet"), Drawn::whole());
    let (left, top, right, bottom) = ink_extent(&met).expect("a filled square draws");
    assert!(
        (right - left - (bottom - top)).abs() <= 2,
        "a meeting square stays square: {left},{top},{right},{bottom}"
    );

    // `none` stretches it to the document's own extent, so the ink is twice as wide as it is tall.
    let stretched = drawn(&mut harness, &document("none"), Drawn::whole());
    let (left, top, right, bottom) = ink_extent(&stretched).expect("a filled square draws");
    let (width, height) = (right - left + 1, bottom - top + 1);
    assert!(
        (width - 2 * height).abs() <= 3,
        "a stretched square is twice as wide as it is tall: {width} by {height}"
    );
}

/// Plain path notation is the same content with nothing else in it, and still draws.
///
/// A drawing written as outlines and a whole document now travel the same way: both become shapes
/// with paint, and the outline's paint is the inherited one. That is a simplification with a
/// regression in it if it is wrong — every icon in every application is path notation — so this
/// draws one through the same source, the same fit and the same emitter and reads it back.
#[test]
fn path_notation_still_draws_in_the_elements_own_colour() {
    let Some(mut harness) = renderer() else {
        return;
    };
    let mut document = Document::new();
    let index = document.append(
        document.document_index(),
        NodeKind::Element,
        ElementName::new("vector"),
    );
    document
        .edit(&zgui_dom::EverythingMatters, |edit| {
            // A diamond in a twenty-four unit square: it fills the middle and leaves the corners.
            edit.set_property(
                index,
                PropKey::new(drawing::PATHS),
                Some(PropValue::from("M12 0 L24 12 L12 24 L0 12 Z")),
            );
            edit.set_property(
                index,
                PropKey::new(drawing::VIEW_BOX),
                Some(PropValue::from("0 0 24 24")),
            );
        })
        .expect("not poisoned");
    let node = document.store().key_of(index);

    let cache = VectorCache::new();
    let drawing = cache
        .frame(&document)
        .drawing(
            node,
            Placement {
                content_box: Rect::new(
                    Point::new(DevicePx(0.0), DevicePx(0.0)),
                    Size::new(DevicePx(EXTENT as f32), DevicePx(EXTENT as f32)),
                ),
                scale: 1.0,
            },
        )
        .expect("the element draws its outlines");

    let mut scene = Scene::new();
    scene.begin_frame(Size::new(EXTENT, EXTENT));
    zgui_paint::emit::vector::draw(
        &mut scene,
        VectorId(1),
        &drawing.shapes,
        ShapePaint {
            fill: Color::srgb(0.0, 0.4, 1.0, 1.0),
            stroke: None,
            stroke_width: 1.0,
        },
        VectorPlacement {
            clip: ClipId::ROOT,
            transform: SpatialId::VIEWPORT,
        },
    );
    scene.finish(&DamageSet::full());
    let pixels = present(&mut harness, &scene);

    assert!(
        is(at(&pixels, 64, 64), [0x00, 0x66, 0xff]),
        "the middle of the diamond is the element's colour, not {:?}",
        at(&pixels, 64, 64)
    );
    assert!(
        !painted(&pixels, 4, 4),
        "and the corners the diamond leaves are still empty, or this drew a box"
    );
    assert_eq!(
        ink_extent(&pixels),
        Some((0, 0, 127, 127)),
        "a diamond fitted to the box reaches every edge of it"
    );
}

/// The fallback rasteriser draws a document's clips and dashes too.
///
/// The seam this whole architecture is arranged around: a document is mapped onto a vector model
/// neither rasteriser owns, so a feature that worked on only one of them would be a feature that
/// disappears when a device without compute shaders runs the same application. Colour ramps are
/// the documented exception — the fallback fills in flat colour — so this asserts the shape rather
/// than the colour, and the case below asserts the exception itself.
#[test]
fn the_fallback_rasteriser_applies_the_same_clips_and_dashes() {
    let Some(mut harness) = harness_at(EXTENT, Which::Coverage) else {
        return;
    };
    const CLIPPED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
      <defs>
        <clipPath id="c"><rect x="32" y="32" width="64" height="64"/></clipPath>
      </defs>
      <g clip-path="url(#c)">
        <rect x="0" y="0" width="128" height="128" fill="#20a040"/>
      </g>
    </svg>"##;
    let clipped = drawn(&mut harness, CLIPPED, Drawn::whole());
    assert!(painted(&clipped, 64, 64), "inside the clip there is ink");
    assert!(!painted(&clipped, 8, 8), "outside it there is none");
    assert_eq!(ink_extent(&clipped), Some((32, 32, 95, 95)));

    const DASHED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
      <path d="M0 64 H128" stroke="#000000" stroke-width="24" stroke-dasharray="16 16"/>
    </svg>"##;
    let dashed = drawn(&mut harness, DASHED, Drawn::whole());
    let covered = (0..EXTENT).filter(|x| painted(&dashed, *x, 64)).count();
    assert!(
        (56..=72).contains(&covered),
        "the fallback has to leave the gaps too, not cover {covered} of {EXTENT}"
    );
}

/// The same display list through a renderer with no rasteriser attached draws nothing.
///
/// The shape of the defect this whole file is written against: the display list is identical,
/// every count is identical, and the surface is empty.
#[test]
fn without_a_rasteriser_the_same_display_list_draws_nothing() {
    let Some(mut attached) = renderer() else {
        return;
    };
    let scene = scene_of(TWO_COLOURS, Drawn::whole());
    let inked = present(&mut attached, &scene);
    assert!(
        ink_extent(&inked).is_some(),
        "with a rasteriser, there is ink"
    );

    let target = RenderTarget::new(Size::new(EXTENT, EXTENT), Scale::new(1.0));
    let Ok(mut bare) = Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false)
    else {
        return;
    };
    assert!(!bare.has_vector_raster());
    let blank = present_unattached(&mut bare, &scene);
    assert!(
        ink_extent(&blank).is_none(),
        "with none, the very same display list composites an empty scratch"
    );
}

/// A document carrying both kinds of fill at once, in three bands across its own width.
///
/// The outer bands are colours the document wrote down and the middle one asked for the inherited
/// colour. Both rules therefore have to hold of the *same* parse of the *same* document, which is
/// what neither a scheme that tints everything nor one that tints nothing can do: the first moves
/// the outer bands and the second leaves the middle one behind.
const BOTH_KINDS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
  <rect x="0" y="0" width="42" height="128" fill="#e01020"/>
  <rect x="42" y="0" width="44" height="128" fill="currentColor"/>
  <rect x="86" y="0" width="42" height="128" fill="#1030d0"/>
</svg>"##;

/// One document keeps its own colours and follows the inherited one, at the same time.
///
/// Asserting the two rules against two documents leaves a scheme that decides per *document* —
/// tinting one whose paints are all alike and keeping one whose paints differ — green on both. It is
/// only a document holding both kinds that says the decision is made per paint.
#[test]
fn one_document_keeps_its_own_colours_while_following_the_inherited_one() {
    let Some(mut harness) = renderer() else {
        return;
    };
    let green = drawn(
        &mut harness,
        BOTH_KINDS,
        Drawn::whole().inheriting(Color::srgb(0.0, 0.8, 0.2, 1.0)),
    );
    let orange = drawn(
        &mut harness,
        BOTH_KINDS,
        Drawn::whole().inheriting(Color::srgb(1.0, 0.5, 0.0, 1.0)),
    );

    for (band, colour) in [(20, [0xe0, 0x10, 0x20]), (110, [0x10, 0x30, 0xd0])] {
        assert!(
            is(at(&green, band, 64), colour),
            "the band the document coloured itself is {colour:?}, not {:?}",
            at(&green, band, 64)
        );
        assert_eq!(
            at(&green, band, 64),
            at(&orange, band, 64),
            "the band at {band} moved when the element's colour did, in a document that also \
             asked for the inherited colour"
        );
    }

    assert!(
        is(at(&green, 64, 64), [0x00, 0xcc, 0x33]),
        "the inherited band takes the element's colour, not {:?}",
        at(&green, 64, 64)
    );
    assert!(
        is(at(&orange, 64, 64), [0xff, 0x80, 0x00]),
        "and follows it when it changes, but it stayed {:?}",
        at(&orange, 64, 64)
    );
}

/// And the fallback rasteriser resolves the same document's colours the same way.
///
/// Colours are decided before either rasteriser sees the shapes, so a difference here would mean
/// one backend re-deciding something that is not its to decide.
#[test]
fn the_fallback_rasteriser_resolves_both_kinds_of_fill_the_same_way() {
    let Some(mut harness) = harness_at(EXTENT, Which::Coverage) else {
        return;
    };
    let green = drawn(
        &mut harness,
        BOTH_KINDS,
        Drawn::whole().inheriting(Color::srgb(0.0, 0.8, 0.2, 1.0)),
    );
    let orange = drawn(
        &mut harness,
        BOTH_KINDS,
        Drawn::whole().inheriting(Color::srgb(1.0, 0.5, 0.0, 1.0)),
    );
    assert!(
        is(at(&green, 20, 64), [0xe0, 0x10, 0x20]),
        "{:?}",
        at(&green, 20, 64)
    );
    assert!(
        is(at(&green, 110, 64), [0x10, 0x30, 0xd0]),
        "{:?}",
        at(&green, 110, 64)
    );
    assert_eq!(at(&green, 20, 64), at(&orange, 20, 64));
    assert_eq!(at(&green, 110, 64), at(&orange, 110, 64));
    assert!(
        is(at(&green, 64, 64), [0x00, 0xcc, 0x33]),
        "{:?}",
        at(&green, 64, 64)
    );
    assert!(
        is(at(&orange, 64, 64), [0xff, 0x80, 0x00]),
        "{:?}",
        at(&orange, 64, 64)
    );
}

/// A ramp the fallback cannot evaluate is filled flat, and is still there.
///
/// The downgrade has to be *flat*, not *absent*. A rasteriser that skips what it cannot evaluate
/// per fragment turns a gradient-filled logo into a hole on every device without compute shaders,
/// and nothing about the display list or the plan says so — the item is there, its ink rectangle is
/// there, the composite reads a scratch nothing wrote. Only the pixels tell the two apart.
#[test]
fn the_fallback_rasteriser_fills_a_ramp_flat_rather_than_leaving_a_hole() {
    let Some(mut harness) = harness_at(EXTENT, Which::Coverage) else {
        return;
    };
    const RAMP: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">
      <defs>
        <linearGradient id="g" x1="0" y1="0" x2="128" y2="0" gradientUnits="userSpaceOnUse">
          <stop offset="0" stop-color="#ff0000"/>
          <stop offset="1" stop-color="#0000ff"/>
        </linearGradient>
      </defs>
      <rect x="0" y="0" width="128" height="128" fill="url(#g)"/>
    </svg>"##;
    let pixels = drawn(&mut harness, RAMP, Drawn::whole());
    assert_eq!(
        ink_extent(&pixels),
        Some((0, 0, 127, 127)),
        "the shape is drawn over its whole box, rather than skipped for the paint it asked for"
    );
    // The mean of the two stops, which is the ramp's average colour along its own line.
    for x in [8, 64, 120] {
        assert!(
            apart(at(&pixels, x, 64), [0x80, 0x00, 0x80]) <= 4,
            "at ({x}, 64) the flat stand-in is the ramp's mean colour, not {:?}",
            at(&pixels, x, 64)
        );
    }
}

/// A frame drawn against the rectangles that changed is the frame drawn whole.
///
/// A document that moved is the case that catches a pass region planned from the wrong rectangle:
/// the drawing has to disappear from where it was as well as appear where it is, and a scissor that
/// covered only one of the two leaves the old one standing.
#[test]
fn a_scissored_frame_of_a_document_is_the_frame_repainted_whole() {
    let Some((mut scissored, mut whole)) = twins(EXTENT, Which::Vello) else {
        return;
    };
    let ink_of = |scene: &Scene| {
        scene
            .primitives
            .vectors
            .iter()
            .map(|item| item.ink)
            .reduce(|union, ink| union.union(ink))
            .expect("a drawing reports ink")
    };

    let before = scene_of(TWO_COLOURS, Drawn::whole().into_box(8.0, 8.0, 48.0, 48.0));
    let first = present(&mut scissored, &before);
    assert!(ink_extent(&first).is_some(), "the first frame draws");
    let _ = present(&mut whole, &before);

    let after = scene_of(TWO_COLOURS, Drawn::whole().into_box(64.0, 56.0, 56.0, 56.0));
    let mut damage = DamageSet::new();
    damage.absorb(whole_pixels(ink_of(&before)));
    damage.absorb(whole_pixels(ink_of(&after)));
    assert!(
        !damage.is_full(),
        "a drawing that moved damages part of the surface, or this compares a repaint with itself"
    );

    let outcome = scissored.renderer.draw(&after, &damage);
    assert!(outcome.retires_damage(), "{outcome:?}");
    let partial = scissored
        .renderer
        .read_presented()
        .expect("a stand-in surface can be read back");
    let repainted = present(&mut whole, &after);
    assert_eq!(
        difference(EXTENT, &partial, &repainted),
        None,
        "a frame drawn against its damage is not the frame drawn whole"
    );
}

/// Draws through a renderer that has no rasteriser, which [`present`] would refuse to do.
fn present_unattached(renderer: &mut WgpuRenderer, scene: &Scene) -> Pixels {
    renderer.draw(scene, &DamageSet::full());
    renderer
        .read_presented()
        .expect("a stand-in surface can be read back")
}
