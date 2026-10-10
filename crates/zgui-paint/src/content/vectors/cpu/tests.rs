use std::sync::Arc;

use smallvec::smallvec;
use zgui_color::Color;
use zgui_scene::kurbo::{self, Affine, BezPath, Point, Rect, Shape as _};
use zgui_scene::peniko;

use super::{LayerJob, RAMP_SAMPLES, VectorPainter, Zeno, ramp};
use crate::emit::vector::ShapePaint;

/// A shape filling `rect` with `paint`.
fn filled(rect: Rect, paint: zgui_svg::Paint) -> zgui_svg::Shape {
    zgui_svg::Shape {
        path: Arc::new(rect.to_path(0.1)),
        fill: Some(zgui_svg::Fill {
            paint,
            rule: peniko::Fill::NonZero,
        }),
        stroke: None,
        clips: Vec::new(),
    }
}

fn solid(color: Color) -> zgui_svg::Paint {
    zgui_svg::Paint::Solid(zgui_svg::Ink::Solid(color))
}

/// A black to white ramp of `kind`.
fn black_to_white(kind: zgui_svg::GradientKind, repeating: bool) -> zgui_svg::Gradient {
    let stops = smallvec![
        zgui_svg::Stop {
            offset: 0.0,
            color: zgui_svg::Ink::Solid(Color::BLACK),
        },
        zgui_svg::Stop {
            offset: 1.0,
            color: zgui_svg::Ink::Solid(Color::WHITE),
        },
    ];
    if repeating {
        zgui_svg::Gradient::repeating(kind, stops)
    } else {
        zgui_svg::Gradient::padded(kind, stops)
    }
}

fn paint() -> ShapePaint {
    ShapePaint {
        fill: Color::BLACK,
        stroke: None,
        stroke_width: 1.0,
    }
}

/// Paints `shapes` through `map` into a `width` by `height` raster.
fn painted(shapes: &[zgui_svg::Shape], map: Affine, width: u32, height: u32) -> Vec<u8> {
    let mut out = vec![0; width as usize * height as usize * 4];
    Zeno::default().paint(
        &LayerJob {
            shapes,
            paint: paint(),
            map,
            stroke_scale: 1.0,
            inherited_stroke: 1.0,
            width,
            height,
        },
        &mut out,
    );
    out
}

fn texel(out: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = (y * width + x) as usize * 4;
    [out[at], out[at + 1], out[at + 2], out[at + 3]]
}

#[test]
fn a_solid_square_fills_whole_pixels_exactly() {
    let shapes = [filled(
        Rect::new(2.0, 2.0, 6.0, 6.0),
        solid(Color::srgb(1.0, 0.0, 0.0, 1.0)),
    )];
    let out = painted(&shapes, Affine::IDENTITY, 8, 8);
    for y in 0..8 {
        for x in 0..8 {
            let inside = (2..6).contains(&x) && (2..6).contains(&y);
            let expected = if inside { [255, 0, 0, 255] } else { [0; 4] };
            assert_eq!(texel(&out, 8, x, y), expected, "at {x}, {y}");
        }
    }
}

#[test]
fn source_over_is_premultiplied() {
    let shapes = [
        filled(
            Rect::new(0.0, 0.0, 4.0, 4.0),
            solid(Color::srgb(1.0, 0.0, 0.0, 1.0)),
        ),
        filled(
            Rect::new(0.0, 0.0, 4.0, 4.0),
            solid(Color::srgb_u8(0, 0, 255, 128)),
        ),
    ];
    let out = painted(&shapes, Affine::IDENTITY, 4, 4);
    // 128 of blue over red keeps 127 of red, and the result stays opaque.
    assert_eq!(texel(&out, 4, 1, 1), [127, 0, 128, 255]);

    let alone = [filled(
        Rect::new(0.0, 0.0, 4.0, 4.0),
        solid(Color::srgb_u8(0, 0, 255, 128)),
    )];
    let out = painted(&alone, Affine::IDENTITY, 4, 4);
    assert_eq!(texel(&out, 4, 1, 1), [0, 0, 128, 128], "premultiplied");
}

#[test]
fn a_linear_ramp_reads_the_vello_ramp() {
    let gradient = black_to_white(
        zgui_svg::GradientKind::Linear {
            start: Point::new(0.0, 0.0),
            end: Point::new(64.0, 0.0),
        },
        false,
    );
    let mut samples = Vec::new();
    ramp(&gradient, Color::BLACK, &mut samples);
    assert_eq!(samples.len(), RAMP_SAMPLES);
    assert_eq!(samples[0], [0, 0, 0, 255]);
    assert_eq!(samples[RAMP_SAMPLES - 1], [255; 4]);
    let middle = (255.0_f32 * 256.0 / 511.0).round() as u8;
    assert_eq!(samples[256][0], middle, "a straight line in sRGB");

    let shapes = [filled(
        Rect::new(0.0, 0.0, 64.0, 1.0),
        zgui_svg::Paint::Gradient(gradient),
    )];
    let out = painted(&shapes, Affine::IDENTITY, 64, 1);
    for x in 0..64 {
        // Read at the texel's corner, as the general rasteriser reads it.
        let t = f64::from(x) / 64.0;
        let index = (t * 511.0).round() as usize;
        let expected = samples[index][0];
        assert_eq!(texel(&out, 64, x, 0), [expected, expected, expected, 255]);
    }
}

#[test]
fn an_elliptic_radial_ramp_pads_and_repeats() {
    let kind = zgui_svg::GradientKind::Radial {
        center: Point::new(16.0, 16.0),
        radius_x: 8.0,
        radius_y: 4.0,
    };
    let at = |repeating: bool, x: u32, y: u32| {
        let shapes = [filled(
            Rect::new(0.0, 0.0, 32.0, 32.0),
            zgui_svg::Paint::Gradient(black_to_white(kind, repeating)),
        )];
        texel(&painted(&shapes, Affine::IDENTITY, 32, 32), 32, x, y)[0]
    };
    let level = |t: f64| (255.0 * (t * 511.0).round() / 511.0).round() as u8;
    // The ramp is measured in units of each axis's own radius.
    assert_eq!(at(false, 19, 15), level((3.0_f64 / 8.0).hypot(1.0 / 4.0)));
    assert_eq!(at(false, 15, 17), level((1.0_f64 / 8.0).hypot(1.0 / 4.0)));
    // Past the ellipse a padded ramp holds its last stop and a repeating one starts again.
    assert_eq!(at(false, 31, 16), 255);
    let t = 15.0_f64 / 8.0;
    assert_eq!(at(true, 31, 16), level(t - t.floor()));
}

#[test]
fn a_clip_cuts_coverage() {
    let mut shape = filled(
        Rect::new(0.0, 0.0, 8.0, 8.0),
        solid(Color::srgb(1.0, 1.0, 1.0, 1.0)),
    );
    shape.clips.push(zgui_svg::Clip {
        path: Arc::new(Rect::new(0.0, 0.0, 4.0, 8.0).to_path(0.1)),
        rule: peniko::Fill::NonZero,
    });
    let out = painted(&[shape], Affine::IDENTITY, 8, 8);
    assert_eq!(texel(&out, 8, 3, 3), [255; 4]);
    assert_eq!(texel(&out, 8, 4, 3), [0; 4]);
}

#[test]
fn a_quarter_pixel_phase_moves_the_edge_coverage() {
    let shapes = [filled(
        Rect::new(4.0, 0.0, 8.0, 4.0),
        solid(Color::srgb(1.0, 1.0, 1.0, 1.0)),
    )];
    let whole = painted(&shapes, Affine::IDENTITY, 10, 4);
    assert_eq!(texel(&whole, 10, 4, 1), [255; 4]);
    let moved = painted(&shapes, Affine::translate((0.25, 0.0)), 10, 4);
    // The edge covers three quarters of the first texel and a quarter of the one past the end.
    let [_, _, _, first] = texel(&moved, 10, 4, 1);
    let [_, _, _, past] = texel(&moved, 10, 8, 1);
    assert!((first as i32 - 191).abs() <= 1, "{first}");
    assert!((past as i32 - 64).abs() <= 1, "{past}");
    assert_eq!(texel(&moved, 10, 3, 1), [0; 4]);
}

#[test]
fn a_stroke_scales_with_the_map() {
    let mut path = BezPath::new();
    path.move_to((2.0, 4.0));
    path.line_to((14.0, 4.0));
    let shape = zgui_svg::Shape {
        path: Arc::new(path),
        fill: None,
        stroke: Some(zgui_svg::Stroke {
            paint: solid(Color::srgb(1.0, 1.0, 1.0, 1.0)),
            style: kurbo::Stroke::new(2.0).with_caps(kurbo::Cap::Butt),
        }),
        clips: Vec::new(),
    };
    let mut out = vec![0; 32 * 16 * 4];
    Zeno::default().paint(
        &LayerJob {
            shapes: &[shape],
            paint: paint(),
            map: Affine::scale(2.0),
            stroke_scale: 2.0,
            inherited_stroke: 1.0,
            width: 32,
            height: 16,
        },
        &mut out,
    );
    // Four texels wide about y = 8.
    assert_eq!(texel(&out, 32, 10, 5), [0; 4]);
    for y in 6..10 {
        assert_eq!(texel(&out, 32, 10, y), [255; 4], "at {y}");
    }
    assert_eq!(texel(&out, 32, 10, 10), [0; 4]);
}

/// The four documents of the static SVG bench.
const SOURCES: [&str; 4] = [
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><linearGradient id="a" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#4f8cff"/><stop offset="1" stop-color="#1b2a5c"/></linearGradient><radialGradient id="b" cx="0.5" cy="0.5" r="0.5"><stop offset="0" stop-color="#ffd166"/><stop offset="1" stop-color="#ef476f"/></radialGradient><clipPath id="c"><circle cx="48" cy="48" r="30"/></clipPath></defs><rect x="4" y="4" width="88" height="88" rx="12" fill="url(#a)"/><circle cx="48" cy="48" r="26" fill="url(#b)"/><g clip-path="url(#c)"><rect x="20" y="40" width="56" height="16" fill="#06d6a0"/></g><path d="M14 80 L30 64 L46 80 Z" fill="#ffffff"/><path d="M60 18 L82 18 L82 26 L60 26 Z" fill="#118ab2"/></svg>"##,
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><linearGradient id="a" x1="0" y1="1" x2="0" y2="0"><stop offset="0" stop-color="#2a9d8f"/><stop offset="1" stop-color="#e9c46a"/></linearGradient><radialGradient id="b" cx="0.3" cy="0.3" r="0.7"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#264653"/></radialGradient><clipPath id="c"><rect x="16" y="16" width="64" height="40"/></clipPath></defs><circle cx="48" cy="48" r="44" fill="url(#b)"/><path d="M8 88 L48 20 L88 88 Z" fill="url(#a)"/><g clip-path="url(#c)"><circle cx="48" cy="56" r="28" fill="#e76f51"/></g><rect x="40" y="70" width="16" height="16" fill="#f4a261"/><path d="M10 10 L22 10 L22 22 Z" fill="#8ecae6"/></svg>"##,
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><linearGradient id="a" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#8338ec"/><stop offset="1" stop-color="#3a86ff"/></linearGradient><radialGradient id="b" cx="0.5" cy="0.5" r="0.5"><stop offset="0" stop-color="#fb5607"/><stop offset="1" stop-color="#ffbe0b"/></radialGradient><clipPath id="c"><path d="M48 8 L88 48 L48 88 L8 48 Z"/></clipPath></defs><rect x="0" y="0" width="96" height="96" fill="#1d1d2c"/><g clip-path="url(#c)"><rect x="0" y="0" width="96" height="96" fill="url(#a)"/></g><circle cx="48" cy="48" r="18" fill="url(#b)"/><path d="M20 20 L32 20 L32 32 L20 32 Z" fill="#ff006e"/><path d="M64 64 L76 64 L76 76 L64 76 Z" fill="#ffffff"/></svg>"##,
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><linearGradient id="a" x1="1" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ef233c"/><stop offset="1" stop-color="#2b2d42"/></linearGradient><radialGradient id="b" cx="0.5" cy="0.6" r="0.5"><stop offset="0" stop-color="#edf2f4"/><stop offset="1" stop-color="#8d99ae"/></radialGradient><clipPath id="c"><circle cx="48" cy="40" r="24"/></clipPath></defs><rect x="8" y="8" width="80" height="80" rx="40" fill="url(#b)"/><g clip-path="url(#c)"><path d="M0 40 L96 40 L96 96 L0 96 Z" fill="url(#a)"/></g><path d="M30 70 L66 70 L66 78 L30 78 Z" fill="#d90429"/><circle cx="20" cy="20" r="6" fill="#2b2d42"/><circle cx="76" cy="20" r="6" fill="#06d6a0"/></svg>"##,
];

/// Measures the painter on the static SVG bench documents at three scales and fits the two cost
/// constants of the layer cache to the times.
///
/// ```text
/// cargo test -p zgui-paint --release calibrate_the_cost_model -- --ignored --nocapture
/// ```
#[test]
#[ignore = "a measurement, run by hand in release"]
fn calibrate_the_cost_model() {
    let mut rows = Vec::new();
    let mut painter = Zeno::default();
    for source in SOURCES {
        let document = zgui_svg::parse(source).expect("a bench document parses");
        let area: f64 = document
            .shapes()
            .iter()
            .map(|shape| shape.path.control_box().area())
            .sum();
        let segments: usize = document
            .shapes()
            .iter()
            .map(|shape| shape.path.elements().len())
            .sum();
        for scale in [1.0, 2.0, 4.0] {
            let side = (96.0 * scale) as u32;
            let mut out = vec![0; side as usize * side as usize * 4];
            let job = LayerJob {
                shapes: document.shapes(),
                paint: paint(),
                map: Affine::scale(scale),
                stroke_scale: scale,
                inherited_stroke: scale,
                width: side,
                height: side,
            };
            painter.paint(&job, &mut out);
            let runs = 20;
            let start = std::time::Instant::now();
            for _ in 0..runs {
                painter.paint(&job, &mut out);
            }
            let us = start.elapsed().as_secs_f64() * 1.0e6 / f64::from(runs);
            let pixels = area * scale * scale;
            rows.push((pixels, segments as f64, us));
            println!("scale {scale}: {pixels:.0} px, {segments} segments, {us:.1} us");
        }
    }
    // Least squares over us = a·pixels + b·segments.
    let (mut pp, mut ps, mut ss, mut pu, mut su) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (p, s, u) in &rows {
        pp += p * p;
        ps += p * s;
        ss += s * s;
        pu += p * u;
        su += s * u;
    }
    let det = pp * ss - ps * ps;
    let a = (pu * ss - su * ps) / det;
    let b = (pp * su - ps * pu) / det;
    println!("US_PER_PIXEL = {a:.5}, US_PER_SEGMENT = {b:.4}");
    for (p, s, u) in &rows {
        println!("  {u:8.1} us measured, {:8.1} us estimated", a * p + b * s);
    }
}
