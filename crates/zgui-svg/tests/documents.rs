//! What a document says, and what comes out of reading it.
//!
//! These are about the mapping rather than about pixels: that a dash array arrives as a dash
//! pattern, that a group transform reaches the geometry, that a colour a document wrote is not
//! confused with one it inherited. What the mapping then *draws* is asserted by reading pixels
//! back, in the rasteriser's own tests.

use zgui_color::Color;
use zgui_svg::{Document, GradientKind, Ink, Paint, Shape, parse};

/// Reads a document, failing the test rather than the process on a bad fixture.
fn read(source: &str) -> Document {
    parse(source).unwrap_or_else(|failure| panic!("reading the fixture: {failure}"))
}

/// A document wrapping `body` in a hundred-unit square space.
fn wrap(body: &str) -> String {
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">{body}</svg>"##)
}

/// The one fill of a document with one shape.
fn only_fill(document: &Document) -> Paint {
    let [shape] = document.shapes() else {
        panic!("expected one shape, found {}", document.shapes().len());
    };
    shape.fill.clone().expect("the shape is filled").paint
}

/// The colour of a solid paint.
fn solid(paint: &Paint, inherited: Color) -> [f32; 3] {
    let Paint::Solid(ink) = paint else {
        panic!("expected a colour, found a ramp");
    };
    ink.resolve(inherited).components()
}

#[test]
fn a_colour_the_document_wrote_is_not_the_one_it_inherits() {
    let own = read(&wrap(
        r##"<rect width="100" height="100" fill="#ff0000"/>"##,
    ));
    assert!(!own.is_inherited());
    assert_eq!(solid(&only_fill(&own), Color::WHITE), [1.0, 0.0, 0.0]);

    let inherited = read(&wrap(
        r##"<rect width="100" height="100" fill="currentColor"/>"##,
    ));
    assert!(inherited.is_inherited());
    assert_eq!(solid(&only_fill(&inherited), Color::WHITE), [1.0, 1.0, 1.0]);
}

/// The case a single-sentinel scheme gets wrong: a document that literally fills with the colour
/// the reader injects must keep it, not follow the element.
#[test]
fn a_literal_fill_is_kept_even_when_it_is_the_colour_the_reader_injects() {
    for sentinel in ["#1f0d3b", "#c4f20e"] {
        let document = read(&wrap(&format!(
            r##"<rect width="100" height="100" fill="{sentinel}"/>"##
        )));
        assert!(
            !document.is_inherited(),
            "{sentinel} is a colour this document wrote down"
        );
    }
}

#[test]
fn a_fill_rule_survives_the_reading() {
    let document = read(&wrap(
        r##"<path fill-rule="evenodd" fill="#000" d="M0 0 H100 V100 H0 Z M20 20 H80 V80 H20 Z"/>"##,
    ));
    let [shape] = document.shapes() else {
        panic!("one path is one shape");
    };
    assert_eq!(shape.fill.as_ref().unwrap().rule, peniko::Fill::EvenOdd);
}

#[test]
fn every_part_of_a_stroke_survives_the_reading() {
    let document = read(&wrap(
        r##"<path d="M0 0 H100" stroke="#000" stroke-width="6" stroke-linecap="square"
                 stroke-linejoin="bevel" stroke-miterlimit="7" stroke-dasharray="4 2"
                 stroke-dashoffset="1"/>"##,
    ));
    let [shape] = document.shapes() else {
        panic!("one path is one shape");
    };
    let stroke = shape.stroke.as_ref().expect("the path is stroked");
    assert_eq!(stroke.style.width, 6.0);
    assert_eq!(stroke.style.start_cap, kurbo::Cap::Square);
    assert_eq!(stroke.style.end_cap, kurbo::Cap::Square);
    assert_eq!(stroke.style.join, kurbo::Join::Bevel);
    assert_eq!(stroke.style.miter_limit, 7.0);
    assert_eq!(stroke.style.dash_pattern.as_slice(), &[4.0, 2.0]);
    assert_eq!(stroke.style.dash_offset, 1.0);
}

/// A group's transform reaches the geometry *and* the thickness, because SVG says a scaled shape
/// is drawn scaled and stroked thicker.
#[test]
fn a_group_transform_reaches_the_geometry_and_the_stroke() {
    let document = read(&wrap(
        r##"<g transform="translate(10 20) scale(2)">
             <path d="M0 0 H10" stroke="#000" stroke-width="3"/>
           </g>"##,
    ));
    let [shape] = document.shapes() else {
        panic!("one path is one shape");
    };
    let bounds = kurbo::Shape::bounding_box(shape.path.as_ref());
    assert_eq!((bounds.x0, bounds.y0), (10.0, 20.0));
    assert_eq!(bounds.x1, 30.0, "ten units at twice the size is twenty");
    assert_eq!(shape.stroke.as_ref().unwrap().style.width, 6.0);
}

#[test]
fn a_group_opacity_is_folded_into_what_its_children_paint_with() {
    let document = read(&wrap(
        r##"<g opacity="0.5">
              <rect width="100" height="100" fill="#ff0000" fill-opacity="0.5"/>
            </g>"##,
    ));
    let Paint::Solid(Ink::Solid(color)) = only_fill(&document) else {
        panic!("a colour under a translucent group is still a colour");
    };
    assert!(
        (color.alpha() - 0.25).abs() < 1.0e-4,
        "a half-opaque fill inside a half-opaque group is a quarter opaque, not {}",
        color.alpha()
    );
}

/// An inherited colour under a translucent group still follows the element, at that opacity.
#[test]
fn a_group_opacity_does_not_turn_an_inherited_colour_into_a_written_one() {
    let document = read(&wrap(
        r##"<g opacity="0.5"><rect width="100" height="100" fill="currentColor"/></g>"##,
    ));
    assert!(document.is_inherited());
    let Paint::Solid(ink) = only_fill(&document) else {
        panic!("a colour is a colour");
    };
    let resolved = ink.resolve(Color::srgb(0.0, 1.0, 0.0, 1.0));
    assert_eq!(resolved.components(), [0.0, 1.0, 0.0]);
    assert!((resolved.alpha() - 0.5).abs() < 1.0e-4);
}

#[test]
fn nested_clips_are_an_intersection_and_a_clip_path_moves_with_its_group() {
    let document = read(&wrap(
        r##"<defs>
             <clipPath id="outer"><rect x="0" y="0" width="50" height="100"/></clipPath>
             <clipPath id="inner"><rect x="0" y="0" width="100" height="50"/></clipPath>
           </defs>
           <g clip-path="url(#outer)" transform="translate(10 0)">
             <g clip-path="url(#inner)">
               <rect width="100" height="100" fill="#000"/>
             </g>
           </g>"##,
    ));
    let [shape] = document.shapes() else {
        panic!("one rectangle is one shape");
    };
    assert_eq!(shape.clips.len(), 2, "two clipped groups is two clips");
    let boxes: Vec<kurbo::Rect> = shape
        .clips
        .iter()
        .map(|clip| kurbo::Shape::bounding_box(clip.path.as_ref()))
        .collect();
    assert!(
        boxes.iter().all(|box_| box_.x0 == 10.0),
        "a clip under a translated group is translated with it: {boxes:?}"
    );
}

/// A `clipPath` with several children keeps what is inside any of them.
#[test]
fn a_clip_path_with_several_children_is_their_union() {
    let document = read(&wrap(
        r##"<defs>
             <clipPath id="c">
               <rect x="0" y="0" width="20" height="20"/>
               <rect x="80" y="80" width="20" height="20"/>
             </clipPath>
           </defs>
           <g clip-path="url(#c)"><rect width="100" height="100" fill="#000"/></g>"##,
    ));
    let [shape] = document.shapes() else {
        panic!("one rectangle is one shape");
    };
    assert_eq!(
        shape.clips.len(),
        1,
        "one clip path is one clip, however many outlines it is drawn with"
    );
    let bounds = kurbo::Shape::bounding_box(shape.clips[0].path.as_ref());
    assert_eq!(
        (bounds.x0, bounds.y0, bounds.x1, bounds.y1),
        (0.0, 0.0, 100.0, 100.0),
        "both children are in the one outline"
    );
    assert_eq!(shape.clips[0].rule, peniko::Fill::NonZero);
}

#[test]
fn a_gradient_arrives_with_its_stops_and_its_geometry_in_the_documents_space() {
    let document = read(&wrap(
        r##"<defs>
              <linearGradient id="g" x1="0" y1="0" x2="100" y2="0"
                              gradientUnits="userSpaceOnUse"
                              gradientTransform="translate(5 0)">
                <stop offset="0" stop-color="#ff0000"/>
                <stop offset="0.5" stop-color="#00ff00" stop-opacity="0.5"/>
                <stop offset="1" stop-color="#0000ff"/>
              </linearGradient>
            </defs>
            <rect width="100" height="100" fill="url(#g)"/>"##,
    ));
    let Paint::Gradient(ramp) = only_fill(&document) else {
        panic!("a paint server is a ramp");
    };
    assert_eq!(
        ramp.kind,
        GradientKind::Linear {
            start: kurbo::Point::new(5.0, 0.0),
            end: kurbo::Point::new(105.0, 0.0),
        },
        "the gradient transform has to reach the ramp's own geometry"
    );
    assert!(!ramp.repeating, "the default spread holds its end colours");
    assert_eq!(ramp.stops.len(), 3);
    let middle = ramp.stops[1].color.resolve(Color::BLACK);
    assert_eq!(middle.components(), [0.0, 1.0, 0.0]);
    assert!(
        (middle.alpha() - 0.5).abs() < 1.0e-4,
        "a stop keeps its own opacity"
    );
}

/// A gradient in bounding-box units is resolved into the shape's own space, so the ramp lands on
/// the shape rather than on the document's origin.
#[test]
fn a_bounding_box_gradient_lands_on_the_shape_it_paints() {
    let document = read(&wrap(
        r##"<defs>
              <linearGradient id="g">
                <stop offset="0" stop-color="#ff0000"/>
                <stop offset="1" stop-color="#0000ff"/>
              </linearGradient>
            </defs>
            <rect x="40" y="10" width="20" height="20" fill="url(#g)"/>"##,
    ));
    let Paint::Gradient(ramp) = only_fill(&document) else {
        panic!("a paint server is a ramp");
    };
    let GradientKind::Linear { start, end } = ramp.kind else {
        panic!("a linear gradient is linear");
    };
    assert!((start.x - 40.0).abs() < 1.0e-4, "{start:?}");
    assert!((end.x - 60.0).abs() < 1.0e-4, "{end:?}");
}

#[test]
fn a_radial_gradient_keeps_its_centre_and_its_radius() {
    let document = read(&wrap(
        r##"<defs>
              <radialGradient id="g" cx="30" cy="40" r="10" gradientUnits="userSpaceOnUse"
                              spreadMethod="repeat">
                <stop offset="0" stop-color="#ff0000"/>
                <stop offset="1" stop-color="#0000ff"/>
              </radialGradient>
            </defs>
            <rect width="100" height="100" fill="url(#g)"/>"##,
    ));
    let Paint::Gradient(ramp) = only_fill(&document) else {
        panic!("a paint server is a ramp");
    };
    assert_eq!(
        ramp.kind,
        GradientKind::Radial {
            center: kurbo::Point::new(30.0, 40.0),
            radius_x: 10.0,
            radius_y: 10.0,
        }
    );
    assert!(ramp.repeating);
}

/// A reflected spread is expressed as a repeating ramp of twice the extent whose second half runs
/// backwards, because there is no reflected spread in the model this framework draws through.
#[test]
fn a_reflected_spread_becomes_a_mirrored_repeating_ramp() {
    let document = read(&wrap(
        r##"<defs>
              <linearGradient id="g" x1="0" y1="0" x2="20" y2="0" spreadMethod="reflect"
                              gradientUnits="userSpaceOnUse">
                <stop offset="0" stop-color="#ff0000"/>
                <stop offset="1" stop-color="#0000ff"/>
              </linearGradient>
            </defs>
            <rect width="100" height="100" fill="url(#g)"/>"##,
    ));
    let Paint::Gradient(ramp) = only_fill(&document) else {
        panic!("a paint server is a ramp");
    };
    assert!(ramp.repeating);
    assert_eq!(
        ramp.kind,
        GradientKind::Linear {
            start: kurbo::Point::new(0.0, 0.0),
            end: kurbo::Point::new(40.0, 0.0),
        },
        "twice the extent, so that one period is there and back"
    );
    let first = ramp.stops.first().unwrap().color.resolve(Color::BLACK);
    let last = ramp.stops.last().unwrap().color.resolve(Color::BLACK);
    assert_eq!(
        first.components(),
        last.components(),
        "and back to where it began"
    );
}

/// The document's own `viewBox` and `preserveAspectRatio` are already in the geometry, so what a
/// caller fits into an element's box is a plain rectangle at the origin.
#[test]
fn the_view_box_is_resolved_into_the_documents_own_extent() {
    let offset = read(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="-10 -10 20 20">
              <rect x="-10" y="-10" width="20" height="20" fill="#000"/>
            </svg>"##,
    );
    assert_eq!(offset.view_box(), [0.0, 0.0, 20.0, 20.0]);
    let bounds = kurbo::Shape::bounding_box(offset.shapes()[0].path.as_ref());
    assert_eq!((bounds.x0, bounds.y0), (0.0, 0.0));

    let stretched = read(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="10" viewBox="0 0 10 10"
                 preserveAspectRatio="none">
              <rect width="10" height="10" fill="#000"/>
            </svg>"##,
    );
    assert_eq!(stretched.view_box(), [0.0, 0.0, 40.0, 10.0]);
    let bounds = kurbo::Shape::bounding_box(stretched.shapes()[0].path.as_ref());
    assert_eq!(
        (bounds.x1, bounds.y1),
        (40.0, 10.0),
        "`none` stretches the square across the document's own extent"
    );
}

/// Everything the model cannot carry is counted rather than dropped in silence.
///
/// Every one of these counters has to be reachable, or it is a field that reports zero for ever
/// and tells a caller nothing — which is worse than not having it, because it reads like an
/// assurance.
#[test]
fn what_a_document_asks_for_that_this_cannot_draw_is_reported() {
    // One transparent pixel, so the image is a real one the reader accepts rather than a broken
    // reference it discards before the model would ever see it.
    let png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJ\
               AAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let document = read(&wrap(&format!(
        r##"<defs>
              <mask id="m"><rect width="50" height="100" fill="#fff"/></mask>
              <filter id="f"><feGaussianBlur stdDeviation="2"/></filter>
              <pattern id="p" width="10" height="10" patternUnits="userSpaceOnUse">
                <rect width="5" height="5" fill="#000"/>
              </pattern>
            </defs>
            <text x="0" y="10">not shaped here</text>
            <image x="0" y="0" width="10" height="10" href="{png}"/>
            <g mask="url(#m)"><rect width="100" height="100" fill="#000"/></g>
            <g filter="url(#f)"><rect width="100" height="100" fill="#000"/></g>
            <g style="mix-blend-mode: multiply">
              <rect width="100" height="100" fill="#000"/>
            </g>
            <rect width="100" height="100" fill="url(#p)"/>"##
    )));
    let unsupported = document.unsupported();
    assert_eq!(unsupported.text, 1, "text is not shaped by this crate");
    assert_eq!(unsupported.images, 1, "a bitmap is not an outline");
    assert_eq!(unsupported.masks, 1);
    assert_eq!(unsupported.filters, 1);
    assert_eq!(unsupported.blend_modes, 1);
    assert_eq!(unsupported.patterns, 1, "a pattern paints nothing here");
    assert!(!unsupported.is_empty());
    assert!(
        !document.shapes().is_empty(),
        "the shapes it can draw are still drawn"
    );
}

#[test]
fn a_document_that_is_not_a_document_is_an_error_rather_than_an_empty_drawing() {
    assert!(parse("this is not markup at all").is_err());
    assert!(parse("").is_err());
}

/// A `use` element and a `symbol` are resolved into outlines before this crate sees them.
#[test]
fn a_reused_shape_arrives_as_its_own_outline() {
    let document = read(&wrap(
        r##"<defs><rect id="r" width="10" height="10" fill="#000"/></defs>
            <use href="#r" x="0" y="0"/>
            <use href="#r" x="50" y="50"/>"##,
    ));
    assert_eq!(document.shapes().len(), 2);
    let where_ = |shape: &Shape| kurbo::Shape::bounding_box(shape.path.as_ref()).x0;
    assert_eq!(where_(&document.shapes()[0]), 0.0);
    assert_eq!(where_(&document.shapes()[1]), 50.0);
}
