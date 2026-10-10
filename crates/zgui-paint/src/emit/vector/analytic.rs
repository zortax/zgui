//! Recognised shapes drawn as quads: the analytic route.
//!
//! A shape whose parts are circles, axis-aligned ellipses, rectangles, rounded rectangles or
//! straight strokes is drawn as rounded, bordered quads, with no raster and no vector item. The
//! quads cover the shape exactly, and their edges are antialiased by the quad shader.
//!
//! A path paints each pixel once however many subpaths cover it, and quads painted one by one
//! do not. So a part of more than one primitive takes this route only while the primitives are at
//! least two device pixels apart, which leaves no pixel that two of them reach.

use zgui_color::Color;
use zgui_geom::{Corners, Device, DevicePx, Point, Rect, Size, Vec2};
use zgui_scene::{ClipId, ClipLink, CornerShape, PaintRef, Quad, Scene, VectorId, kurbo};

use super::document::{density_of, reference, stroke_of};
use super::recognise::{self, Decomposition, Limits, Part};
use super::{ShapePaint, VectorPlacement};
use crate::content::vectors::VectorMaskSource;

/// The most primitives one part of a shape may become.
const MAX_PRIMS: usize = 256;

/// How many path elements make a failed recognition worth remembering.
///
/// A small path fails fast, and it is tried again on every frame. A large one is not tried again
/// for a few frames.
const REMEMBERED: usize = 64;

/// The smallest scale either axis of the transform may have.
///
/// A quad's geometry reaches one unit of its own space past its bounds, and that unit has to
/// hold the half pixel the antialiased edge spreads over.
const MIN_SCALE: f32 = 0.75;

/// How far apart, in device pixels, each primitive's bounds are kept from every other's on each
/// side.
const MARGIN: f32 = 1.0;

/// One quad before its paints are interned.
#[derive(Clone, Copy, Debug)]
struct Prim {
    /// The outer edge, as `[x0, y0, x1, y1]`.
    rect: [f32; 4],
    /// The corner radii in quad order.
    radii: [f32; 8],
    /// The superellipse exponent of the corners.
    exponent: f32,
    /// The border width, or 0 for a quad that is only filled.
    border: f32,
}

impl Prim {
    /// A disc or a ring of `outer` radius, with a border down to `inner`.
    fn disc([cx, cy, outer, inner]: [f32; 4]) -> Self {
        Self {
            rect: [cx - outer, cy - outer, cx + outer, cy + outer],
            radii: [outer; 8],
            exponent: CornerShape::ROUND.get(),
            border: if inner > 0.0 { outer - inner } else { 0.0 },
        }
    }

    /// A stroked axis-aligned segment, or `None` for a diagonal one.
    fn capsule(capsule: [f32; 4], caps: u8, half: f32, tau: f32) -> Option<Self> {
        let [x0, y0, x1, y1] = capsule;
        let reach = |cap: u8| if cap == recognise::BUTT { 0.0 } else { half };
        let round = |cap: u8| if cap == recognise::ROUND { half } else { 0.0 };
        let (start, end) = (caps & 3, caps >> 2);
        if (y1 - y0).abs() <= tau && (x1 - x0).abs() >= (y1 - y0).abs() {
            let y = (y0 + y1) / 2.0;
            // The end with the smaller coordinate is the left one.
            let (left, right) = if x0 <= x1 { (start, end) } else { (end, start) };
            let (l, r) = (round(left), round(right));
            Some(Self {
                rect: [
                    x0.min(x1) - reach(left),
                    y - half,
                    x0.max(x1) + reach(right),
                    y + half,
                ],
                radii: [l, l, r, r, r, r, l, l],
                exponent: CornerShape::ROUND.get(),
                border: 0.0,
            })
        } else if (x1 - x0).abs() <= tau {
            let x = (x0 + x1) / 2.0;
            let (top, bottom) = if y0 <= y1 { (start, end) } else { (end, start) };
            let (t, b) = (round(top), round(bottom));
            Some(Self {
                rect: [
                    x - half,
                    y0.min(y1) - reach(top),
                    x + half,
                    y0.max(y1) + reach(bottom),
                ],
                radii: [t, t, t, t, b, b, b, b],
                exponent: CornerShape::ROUND.get(),
                border: 0.0,
            })
        } else {
            None
        }
    }

    /// The rectangle, in the shape's own space.
    fn bounds(&self) -> Rect<DevicePx, Device> {
        let [x0, y0, x1, y1] = self.rect;
        Rect::new(
            Point::new(DevicePx(x0), DevicePx(y0)),
            Size::new(DevicePx(x1 - x0), DevicePx(y1 - y0)),
        )
    }

    /// The quad drawing this with `fill` inside and `stroke` on its border.
    fn quad(&self, fill: PaintRef, stroke: PaintRef, placement: (ClipId, u32)) -> Quad {
        let mut quad = Quad::filled(self.bounds(), fill).clipped(placement.0);
        quad.transform = placement.1;
        quad.radii = self.radii;
        quad.shape = self.exponent;
        quad.border = [self.border; 4];
        quad.stroke = stroke;
        quad
    }
}

/// The primitives of one decomposition, or `None` when one of them is no quad.
fn prims(found: &Decomposition, tau: f32) -> Option<Vec<Prim>> {
    let mut prims = Vec::with_capacity(found.count);
    prims.extend(found.discs.iter().map(|disc| Prim::disc(*disc)));
    prims.extend(found.boxes.iter().map(|prim| Prim {
        rect: prim.rect,
        radii: prim.radii,
        exponent: prim.exponent,
        border: prim.border,
    }));
    for (capsule, caps) in found.capsules.iter().zip(&found.caps) {
        prims.push(Prim::capsule(*capsule, *caps, found.half_width, tau)?);
    }
    Some(prims)
}

/// How one part of a shape is painted, read before anything is interned.
#[derive(Clone, Copy, Debug)]
struct Look {
    /// Whether it paints anything at all.
    visible: bool,
    /// Whether it hides everything under it.
    opaque: bool,
}

/// How `paint` looks when it inherits `inherited`.
fn look(paint: &zgui_svg::Paint, inherited: Color) -> Look {
    match paint {
        zgui_svg::Paint::Solid(ink) => of_color(ink.resolve(inherited)),
        zgui_svg::Paint::Gradient(ramp) => Look {
            visible: true,
            opaque: ramp
                .stops
                .iter()
                .all(|stop| stop.color.resolve(inherited).alpha() >= 1.0),
        },
    }
}

/// How one colour looks.
fn of_color(color: Color) -> Look {
    Look {
        visible: color.alpha() != 0.0,
        opaque: color.alpha() >= 1.0,
    }
}

/// Emits a shape whose parts are recognised analytic shapes as quads, or declines the route.
///
/// Nothing is pushed unless every part and every clip is recognised, and the primitives of each
/// part are apart.
pub(super) fn emit_analytic(
    scene: &mut Scene,
    id: VectorId,
    shape: &zgui_svg::Shape,
    paint: &ShapePaint,
    masks: &dyn VectorMaskSource,
    placement: VectorPlacement,
) -> Option<usize> {
    if !masks.analytic(id) {
        return None;
    }
    let affine = scene
        .spatial
        .resolve(placement.transform)
        .as_ref()
        .and_then(zgui_geom::Matrix4::to_affine2)?;
    // Axis-aligned and the same scale on both axes: the quad shader measures one pixel as the
    // larger of its two axis steps, which is exact only there.
    let [kx, ky] = density_of(&affine, true)?;
    if kx < MIN_SCALE || ky < MIN_SCALE {
        return None;
    }
    let fill = shape
        .fill
        .as_ref()
        .filter(|fill| look(&fill.paint, paint.fill).visible);
    let inherited_stroke = (shape.stroke.is_none() && paint.stroke.is_some())
        .then(|| kurbo::Stroke::new(f64::from(paint.stroke_width)));
    let outline = match &shape.stroke {
        Some(stroke) => {
            let look = look(&stroke.paint, paint.stroke.unwrap_or(paint.fill));
            look.visible.then_some((&stroke.style, look.opaque))
        }
        None => paint
            .stroke
            .map(of_color)
            .filter(|look| look.visible)
            .zip(inherited_stroke.as_ref())
            .map(|(look, style)| (style, look.opaque)),
    };
    if fill.is_none() && outline.is_none() {
        return None;
    }

    let tau = recognise::tau(&affine)?;
    let limits = Limits {
        tau,
        max_prims: MAX_PRIMS,
    };
    let declined = || {
        if shape.path.elements().len() >= REMEMBERED {
            masks.analytic_declined(id);
        }
        None
    };
    let filled = match fill {
        Some(fill) => match recognise::recognise(&shape.path, Part::Fill(fill.rule), limits) {
            Some(found) => Some(found),
            None => return declined(),
        },
        None => None,
    };
    let stroked = match outline {
        Some((style, _)) => match recognise::recognise(&shape.path, Part::Stroke(style), limits) {
            Some(found) => Some(found),
            None => return declined(),
        },
        None => None,
    };
    // A part with no primitives encloses no area and draws nothing.
    let filled = filled.filter(|found| found.count > 0);
    let stroked = stroked.filter(|found| found.count > 0);
    if filled.is_none() && stroked.is_none() {
        return None;
    }
    let tau = tau as f32;
    let fills = match &filled {
        Some(found) => prims(found, tau)?,
        None => Vec::new(),
    };
    let strokes = match &stroked {
        Some(found) => prims(found, tau)?,
        None => Vec::new(),
    };

    // Each part on its own: a fill and a stroke of one subpath always overlap, and draw in the
    // order the general route draws them.
    let on_device = |prims: &[Prim]| -> Vec<Rect<DevicePx, Device>> {
        prims
            .iter()
            .map(|prim| affine.transform_rect(prim.bounds()))
            .collect()
    };
    let device_fills = on_device(&fills);
    let device_strokes = on_device(&strokes);
    for device in [&device_fills, &device_strokes] {
        let rects: Vec<[f32; 4]> = device
            .iter()
            .map(|rect| [rect.left().0, rect.top().0, rect.right().0, rect.bottom().0])
            .collect();
        if !recognise::separated(&rects, MARGIN) {
            return None;
        }
    }

    let mut clips = Vec::with_capacity(shape.clips.len());
    for clip in &shape.clips {
        let found = recognise::recognise(&clip.path, Part::Fill(clip.rule), limits)?;
        let [prim] = found.boxes.as_slice() else {
            return None;
        };
        if found.count != 1 || prim.exponent != CornerShape::ROUND.get() {
            return None;
        }
        clips.push(*prim);
    }

    // Everything is decided. From here on, the shape is drawn.
    let mut clip = placement.clip;
    for prim in clips {
        let [x0, y0, x1, y1] = prim.rect;
        let radius = |at: usize| Vec2::new(DevicePx(prim.radii[at]), DevicePx(prim.radii[at + 1]));
        let link = ClipLink::shaped(
            Rect::new(
                Point::new(DevicePx(x0), DevicePx(y0)),
                Size::new(DevicePx(x1 - x0), DevicePx(y1 - y0)),
            ),
            Corners {
                top_left: radius(0),
                top_right: radius(2),
                bottom_right: radius(4),
                bottom_left: radius(6),
            },
            CornerShape::ROUND,
            placement.transform,
        );
        clip = scene.clips.push(clip, link);
        scene.note_minted_clip(clip);
    }
    let space = (clip, placement.transform.index());
    let fill_paint = match fill {
        Some(fill) => reference(scene, &fill.paint, paint.fill),
        None => PaintRef::NONE,
    };
    let stroke_paint = match stroked {
        Some(_) => stroke_of(scene, shape, paint).map_or(PaintRef::NONE, |stroke| stroke.paint),
        None => PaintRef::NONE,
    };

    // One subpath filled and stroked with a paint that hides what is under it is one quad: a
    // quad paints its fill under its border, and the border covers it there.
    let opaque = outline.is_some_and(|(_, opaque)| opaque);
    if let ([_], [border]) = (fills.as_slice(), strokes.as_slice())
        && border.border > 0.0
        && opaque
    {
        let quad = border.quad(fill_paint, stroke_paint, space);
        return Some(usize::from(scene.push_quad(quad).is_some()));
    }

    let fill_quads = fills.iter().zip(&device_fills).map(|(prim, device)| {
        (
            device.left().0,
            prim.quad(fill_paint, PaintRef::NONE, space),
        )
    });
    let stroke_quads = strokes.iter().zip(&device_strokes).map(|(prim, device)| {
        let quad = if prim.border > 0.0 {
            prim.quad(PaintRef::NONE, stroke_paint, space)
        } else {
            prim.quad(stroke_paint, PaintRef::NONE, space)
        };
        (device.left().0, quad)
    });
    let mut pushed = run(scene, fill_quads.collect(), space);
    pushed += run(scene, stroke_quads.collect(), space);
    Some(pushed)
}

/// Pushes `quads` left to right as one run, and returns how many survived.
fn run(scene: &mut Scene, mut quads: Vec<(f32, Quad)>, (clip, space): (ClipId, u32)) -> usize {
    quads.sort_by(|one, two| one.0.total_cmp(&two.0));
    let inks: Vec<_> = quads.iter().map(|(_, quad)| quad.ink()).collect();
    scene.begin_run(&inks, clip, space);
    let mut pushed = 0;
    for (_, quad) in quads {
        pushed += usize::from(scene.push_quad(quad).is_some());
    }
    scene.end_run();
    pushed
}
