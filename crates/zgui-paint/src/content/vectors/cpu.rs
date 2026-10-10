//! A whole drawing painted on the CPU into one premultiplied sRGB tile.
//!
//! The painter follows the general rasteriser where the two can differ. A gradient reads a ramp of
//! 512 samples interpolated premultiplied in sRGB, built the way vello builds its ramps. Colours are
//! quantised to eight bits before they are blended, as the general route hands them over. Shapes
//! composite source-over in gamma space, the fill before the stroke. Coverage comes from zeno, built
//! the way the mask route builds it.

use zgui_color::{Color, ColorSpace};
use zgui_scene::kurbo::{self, Affine, PathEl, Point, Rect};
use zgui_scene::peniko;

use crate::emit::vector::ShapePaint;

#[cfg(test)]
mod tests;

/// Paints one drawing into a tile.
///
/// A trait so that another CPU rasteriser can take the place of zeno.
pub(crate) trait VectorPainter {
    /// Paints `job` into `out`: premultiplied sRGB RGBA8, `width * height * 4` bytes, top row
    /// first.
    fn paint(&mut self, job: &LayerJob<'_>, out: &mut [u8]);
}

/// One drawing to paint, and the raster it is painted into.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LayerJob<'a> {
    /// The shapes, in their own space and in painting order.
    pub(crate) shapes: &'a [zgui_svg::Shape],
    /// What an inherited paint resolves to, before any folded opacity.
    pub(crate) paint: ShapePaint,
    /// Path space to raster texels.
    pub(crate) map: Affine,
    /// The uniform scale of the linear part of `map`, which multiplies a shape's own stroke. One
    /// when nothing strokes.
    pub(crate) stroke_scale: f64,
    /// The width of the element's stroke, in raster texels.
    pub(crate) inherited_stroke: f64,
    /// The raster's width in texels.
    pub(crate) width: u32,
    /// The raster's height in texels.
    pub(crate) height: u32,
}

/// The zeno painter, with scratch that one job leaves for the next.
#[derive(Default)]
pub(crate) struct Zeno {
    /// The coverage of the part being painted.
    coverage: Vec<u8>,
    /// The coverage of one clip.
    clip: Vec<u8>,
    /// The ramp of the gradient being painted, premultiplied.
    ramp: Vec<[u8; 4]>,
    /// The commands of the outline being rasterised, in texels of the part's rectangle.
    commands: Vec<zeno::Command>,
}

impl core::fmt::Debug for Zeno {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Zeno").finish_non_exhaustive()
    }
}

/// How many samples a ramp holds.
pub(crate) const RAMP_SAMPLES: usize = 512;

/// One part of a shape: what covers it and what paints it.
struct Part<'a> {
    /// The outline.
    path: &'a kurbo::BezPath,
    /// How the outline becomes coverage.
    style: PartStyle,
    /// What paints it.
    paint: Shading<'a>,
}

/// How an outline becomes coverage, in texels.
enum PartStyle {
    /// Its interior under a fill rule.
    Fill(peniko::Fill),
    /// The outline of this stroke, already in texels.
    Stroke(kurbo::Stroke),
}

/// What paints a part.
#[derive(Clone, Copy)]
enum Shading<'a> {
    /// One colour.
    Solid(Color),
    /// A ramp, with the colour an inherited stop resolves to.
    Gradient(&'a zgui_svg::Gradient, Color),
}

impl VectorPainter for Zeno {
    fn paint(&mut self, job: &LayerJob<'_>, out: &mut [u8]) {
        out.fill(0);
        let layer = Rect::new(0.0, 0.0, f64::from(job.width), f64::from(job.height));
        let inverse = job.map.inverse();
        for shape in job.shapes {
            // Every clip bounds the shape, so the parts are cut to the clips before anything is
            // rasterised.
            let mut bounds = layer;
            for clip in &shape.clips {
                bounds = bounds.intersect(job.map.transform_rect_bbox(clip.path.control_box()));
            }
            if let Some(fill) = &shape.fill {
                let part = Part {
                    path: &shape.path,
                    style: PartStyle::Fill(fill.rule),
                    paint: shading(&fill.paint, job.paint.fill),
                };
                self.part(job, shape, &part, bounds, inverse, out);
            }
            let stroke = match &shape.stroke {
                Some(stroke) => Some(Part {
                    path: &shape.path,
                    style: PartStyle::Stroke(zgui_svg::document::place::scaled(
                        &stroke.style,
                        job.stroke_scale,
                    )),
                    paint: shading(&stroke.paint, job.paint.stroke.unwrap_or(job.paint.fill)),
                }),
                None => job.paint.stroke.map(|color| Part {
                    path: &shape.path,
                    style: PartStyle::Stroke(kurbo::Stroke::new(job.inherited_stroke)),
                    paint: Shading::Solid(color),
                }),
            };
            if let Some(part) = stroke {
                self.part(job, shape, &part, bounds, inverse, out);
            }
        }
    }
}

/// What paints `paint`, given the colour an inherited ink resolves to.
fn shading(paint: &zgui_svg::Paint, inherited: Color) -> Shading<'_> {
    match paint {
        zgui_svg::Paint::Solid(ink) => Shading::Solid(ink.resolve(inherited)),
        zgui_svg::Paint::Gradient(gradient) => Shading::Gradient(gradient, inherited),
    }
}

impl Zeno {
    /// Paints one part of `shape` over `out`, inside `bounds`.
    fn part(
        &mut self,
        job: &LayerJob<'_>,
        shape: &zgui_svg::Shape,
        part: &Part<'_>,
        bounds: Rect,
        inverse: Affine,
        out: &mut [u8],
    ) {
        let reach = match &part.style {
            PartStyle::Fill(_) => 0.0,
            PartStyle::Stroke(stroke) => {
                if !(stroke.width > 0.0 && stroke.width.is_finite()) {
                    return;
                }
                reach(stroke)
            }
        };
        let ink = job
            .map
            .transform_rect_bbox(part.path.control_box())
            .inflate(reach, reach)
            .intersect(bounds);
        let x0 = ink.x0.floor().max(0.0);
        let y0 = ink.y0.floor().max(0.0);
        let x1 = ink.x1.ceil().min(f64::from(job.width));
        let y1 = ink.y1.ceil().min(f64::from(job.height));
        if !(x1 > x0 && y1 > y0) {
            return;
        }
        let origin = Point::new(x0, y0);
        let (width, height) = ((x1 - x0) as u32, (y1 - y0) as u32);
        let texels = width as usize * height as usize;

        let map = Affine::translate(-origin.to_vec2()) * job.map;
        commands(part.path, map, &mut self.commands);
        self.coverage.clear();
        self.coverage.resize(texels, 0);
        render(&self.commands, &part.style, width, height, &mut self.coverage);
        for clip in &shape.clips {
            commands(&clip.path, map, &mut self.commands);
            self.clip.clear();
            self.clip.resize(texels, 0);
            render(
                &self.commands,
                &PartStyle::Fill(clip.rule),
                width,
                height,
                &mut self.clip,
            );
            for (coverage, clip) in self.coverage.iter_mut().zip(&self.clip) {
                *coverage = multiply(*coverage, *clip);
            }
        }

        let row = job.width as usize * 4;
        let (left, top) = (x0 as usize, y0 as usize);
        match part.paint {
            Shading::Solid(color) => {
                let source = premultiplied(color);
                for y in 0..height as usize {
                    let line = &self.coverage[y * width as usize..][..width as usize];
                    let target = &mut out[(top + y) * row + left * 4..][..width as usize * 4];
                    for (x, &coverage) in line.iter().enumerate() {
                        if coverage != 0 {
                            over(&mut target[x * 4..x * 4 + 4], source, coverage);
                        }
                    }
                }
            }
            Shading::Gradient(gradient, inherited) => {
                ramp(gradient, inherited, &mut self.ramp);
                let measure = Measure::of(gradient, inverse);
                let last = (RAMP_SAMPLES - 1) as f32;
                for y in 0..height as usize {
                    let line = &self.coverage[y * width as usize..][..width as usize];
                    let target = &mut out[(top + y) * row + left * 4..][..width as usize * 4];
                    let centre_y = (top + y) as f32 + 0.5;
                    for (x, &coverage) in line.iter().enumerate() {
                        if coverage == 0 {
                            continue;
                        }
                        let centre = [(left + x) as f32 + 0.5, centre_y];
                        let t = extend(measure.at(centre), gradient.repeating);
                        let index = ((t * last + 0.5) as usize).min(RAMP_SAMPLES - 1);
                        over(&mut target[x * 4..x * 4 + 4], self.ramp[index], coverage);
                    }
                }
            }
        }
    }
}

/// How far a stroke's outline reaches past the path, at most.
fn reach(stroke: &kurbo::Stroke) -> f64 {
    let half = stroke.width.max(0.0) / 2.0;
    let miter = match stroke.join {
        kurbo::Join::Miter => stroke.miter_limit.max(1.0),
        kurbo::Join::Round | kurbo::Join::Bevel => 1.0,
    };
    let squared = |cap| cap == kurbo::Cap::Square;
    let cap = if squared(stroke.start_cap) || squared(stroke.end_cap) {
        core::f64::consts::SQRT_2
    } else {
        1.0
    };
    // One texel more, for the coverage the rasteriser spreads over the edge.
    half * miter.max(cap) + 1.0
}

/// The outline's commands through `map`, as zeno reads them.
fn commands(path: &kurbo::BezPath, map: Affine, out: &mut Vec<zeno::Command>) {
    out.clear();
    let point = |point: Point| {
        let point = map * point;
        zeno::Point::new(point.x as f32, point.y as f32)
    };
    out.extend(path.elements().iter().map(|element| match *element {
        PathEl::MoveTo(p) => zeno::Command::MoveTo(point(p)),
        PathEl::LineTo(p) => zeno::Command::LineTo(point(p)),
        PathEl::QuadTo(a, b) => zeno::Command::QuadTo(point(a), point(b)),
        PathEl::CurveTo(a, b, c) => zeno::Command::CurveTo(point(a), point(b), point(c)),
        PathEl::ClosePath => zeno::Command::Close,
    }));
}

/// Rasterises `commands` into `out`, which holds `width * height` zeroed texels.
fn render(commands: &[zeno::Command], style: &PartStyle, width: u32, height: u32, out: &mut [u8]) {
    let mut mask = zeno::Mask::new(commands);
    let dashes: Vec<f32>;
    match style {
        PartStyle::Fill(rule) => {
            mask.style(match rule {
                peniko::Fill::NonZero => zeno::Fill::NonZero,
                peniko::Fill::EvenOdd => zeno::Fill::EvenOdd,
            });
        }
        PartStyle::Stroke(stroke) => {
            dashes = stroke.dash_pattern.iter().map(|dash| *dash as f32).collect();
            mask.style(zeno::Stroke {
                width: stroke.width as f32,
                join: match stroke.join {
                    kurbo::Join::Bevel => zeno::Join::Bevel,
                    kurbo::Join::Miter => zeno::Join::Miter,
                    kurbo::Join::Round => zeno::Join::Round,
                },
                miter_limit: stroke.miter_limit as f32,
                start_cap: cap(stroke.start_cap),
                end_cap: cap(stroke.end_cap),
                dashes: &dashes,
                offset: stroke.dash_offset as f32,
                scale: true,
            });
        }
    }
    mask.size(width, height);
    mask.render_into(out, None);
}

fn cap(cap: kurbo::Cap) -> zeno::Cap {
    match cap {
        kurbo::Cap::Butt => zeno::Cap::Butt,
        kurbo::Cap::Square => zeno::Cap::Square,
        kurbo::Cap::Round => zeno::Cap::Round,
    }
}

/// `a · b / 255`, rounded.
fn multiply(a: u8, b: u8) -> u8 {
    let product = u32::from(a) * u32::from(b) + 128;
    ((product + (product >> 8)) >> 8) as u8
}

/// Composites premultiplied `source` over the texel `target` at `coverage`: source-over in gamma
/// space, `source · coverage + target · (1 − source.a · coverage)`, in bytes with rounding.
fn over(target: &mut [u8], source: [u8; 4], coverage: u8) {
    if coverage == 255 && source[3] == 255 {
        target.copy_from_slice(&source);
        return;
    }
    let keep = 255 - multiply(source[3], coverage);
    for (channel, value) in target.iter_mut().zip(source) {
        *channel = multiply(value, coverage).saturating_add(multiply(*channel, keep));
    }
}

/// One colour as the general route hands it over, straight sRGB bytes, premultiplied into bytes.
pub(crate) fn premultiplied(color: Color) -> [u8; 4] {
    premultiply(straight(color))
}

/// Straight, gamma-encoded sRGB, from 0 to 1, quantised to bytes.
fn straight(color: Color) -> [f32; 4] {
    let srgb = color.to_space(ColorSpace::Srgb);
    let [red, green, blue] = srgb.components();
    let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() / 255.0;
    [byte(red), byte(green), byte(blue), byte(srgb.alpha())]
}

/// Straight colour premultiplied, rounded to bytes.
fn premultiply([red, green, blue, alpha]: [f32; 4]) -> [u8; 4] {
    bytes([red * alpha, green * alpha, blue * alpha, alpha])
}

/// Values from 0 to 1 as rounded bytes.
fn bytes(values: [f32; 4]) -> [u8; 4] {
    values.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// The ramp of `gradient` into `out`: [`RAMP_SAMPLES`] premultiplied samples, quantised to bytes.
///
/// The construction is vello's `make_ramp`: each sample lerps the two stops around it,
/// premultiplied, in sRGB. The first stop is read as if it stood at offset 0.
pub(crate) fn ramp(gradient: &zgui_svg::Gradient, inherited: Color, out: &mut Vec<[u8; 4]>) {
    out.clear();
    let stops = &gradient.stops;
    let Some(first) = stops.first() else {
        out.resize(RAMP_SAMPLES, [0; 4]);
        return;
    };
    // Premultiplied in floating point, as the lerp reads them.
    let color = |ink: zgui_svg::Ink| {
        let [red, green, blue, alpha] = straight(ink.resolve(inherited));
        [red * alpha, green * alpha, blue * alpha, alpha]
    };
    let mut last_u = 0.0_f32;
    let mut last_c = color(first.color);
    let mut this_u = last_u;
    let mut this_c = last_c;
    let mut next = 0;
    for index in 0..RAMP_SAMPLES {
        let u = index as f32 / (RAMP_SAMPLES - 1) as f32;
        while u > this_u {
            last_u = this_u;
            last_c = this_c;
            if let Some(stop) = stops.get(next + 1) {
                this_u = stop.offset;
                this_c = color(stop.color);
                next += 1;
            } else {
                break;
            }
        }
        let du = this_u - last_u;
        let sample: [f32; 4] = if du < 1.0e-9 {
            this_c
        } else {
            let t = (u - last_u) / du;
            core::array::from_fn(|channel| last_c[channel] + t * (this_c[channel] - last_c[channel]))
        };
        out.push(bytes(sample));
    }
}

/// Where a ramp is read at a texel centre: an affine function of the centre, and for a radial
/// ramp the length of two of them.
struct Measure {
    /// `t`, or the first radial axis, as `a·x + b·y + c`.
    first: [f32; 3],
    /// The second radial axis, for a radial ramp.
    second: Option<[f32; 3]>,
}

impl Measure {
    /// The measure of `gradient`, read through `inverse` from texels to path space.
    fn of(gradient: &zgui_svg::Gradient, inverse: Affine) -> Self {
        let [a, b, c, d, e, f] = inverse.as_coeffs();
        // A path-space function `p·x' + q·y' + r` read at the texel centre.
        let through = |p: f64, q: f64, r: f64| {
            [p * a + q * b, p * c + q * d, p * e + q * f + r].map(|value| value as f32)
        };
        match gradient.kind {
            zgui_svg::GradientKind::Linear { start, end } => {
                let along = end - start;
                let length = along.hypot2();
                if length <= 0.0 || !length.is_finite() {
                    return Self {
                        first: [0.0, 0.0, 1.0],
                        second: None,
                    };
                }
                let (p, q) = (along.x / length, along.y / length);
                Self {
                    first: through(p, q, -(p * start.x + q * start.y)),
                    second: None,
                }
            }
            zgui_svg::GradientKind::Radial {
                center,
                radius_x,
                radius_y,
            } => {
                let rx = radius_x.max(f64::from(f32::MIN_POSITIVE));
                let ry = radius_y.max(f64::from(f32::MIN_POSITIVE));
                Self {
                    first: through(1.0 / rx, 0.0, -center.x / rx),
                    second: Some(through(0.0, 1.0 / ry, -center.y / ry)),
                }
            }
        }
    }

    /// `t` at `centre`.
    fn at(&self, [x, y]: [f32; 2]) -> f32 {
        let value = |[a, b, c]: [f32; 3]| a * x + b * y + c;
        match self.second {
            None => value(self.first),
            Some(second) => {
                let (u, v) = (value(self.first), value(second));
                (u * u + v * v).sqrt()
            }
        }
    }
}

/// `t` padded to the ends, or repeated.
fn extend(t: f32, repeating: bool) -> f32 {
    if !t.is_finite() {
        return 0.0;
    }
    if repeating {
        t - t.floor()
    } else {
        t.clamp(0.0, 1.0)
    }
}
