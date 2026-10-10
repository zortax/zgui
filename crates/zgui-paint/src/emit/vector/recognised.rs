//! Recognition of a shape from its source, through the fit that places it, with a cache.
//!
//! A canvas holds its shapes in its own units and a fit places them in the fragment. Recognising
//! the source path and moving the result costs as many operations as there are prims; placing the
//! path first costs a copy of every element. Under a uniform fit the two give the same prims, so
//! the source is what is recognised.

use std::sync::Arc;

use zgui_scene::kurbo::{self, Affine, BezPath, Vec2};
use zgui_scene::peniko;

use super::ShapeSource;
use super::recognise::{self, Declined, Decomposition, Limits, Part};
use crate::content::vectors::{PartKey, VectorMaskSource};

/// Which outline of a shape is recognised.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Outline {
    /// The shape's own path.
    Path,
    /// One of its clips, by index.
    Clip(usize),
}

/// Which part of an outline is recognised, before the fit.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PartOf {
    /// The interior under a fill rule.
    Fill(peniko::Fill),
    /// The outline the shape's own stroke style draws.
    OwnStroke,
    /// The outline an inherited stroke of this width, in fragment units, draws.
    Inherited(f64),
}

/// The scale and the offset of a fit that scales both axes alike, or `None` for any other fit.
pub(crate) fn uniform(fit: Affine) -> Option<(f64, Vec2)> {
    let [a, b, c, d, x, y] = fit.as_coeffs();
    (a > 0.0 && b == 0.0 && c == 0.0 && d == a).then_some((a, Vec2::new(x, y)))
}

/// The path, the scale and the offset recognition reads for `outline` of `source`.
///
/// The source path under a uniform fit, and the placed path under any other.
fn read<'a>(
    source: &ShapeSource<'a>,
    outline: Outline,
) -> Option<(&'a zgui_svg::Shape, &'a Arc<BezPath>, f64, Vec2)> {
    let (shape, scale, offset) = match uniform(source.fit) {
        Some((scale, offset)) => (source.shape, scale, offset),
        None => (source.placed(), 1.0, Vec2::ZERO),
    };
    let path = match outline {
        Outline::Path => &shape.path,
        Outline::Clip(index) => &shape.clips.get(index)?.path,
    };
    Some((shape, path, scale, offset))
}

/// Whether every line of the shape's path runs along an axis within a quarter of `tau_local`, and
/// no segment is a quadratic.
///
/// A uniform fit keeps an axis an axis, so the source path answers for the placed one.
pub(crate) fn straight(source: &ShapeSource<'_>, tau_local: f64) -> bool {
    match read(source, Outline::Path) {
        Some((_, path, scale, _)) => straight_on_axes(path, tau_local / scale),
        None => false,
    }
}

/// What `part` of `outline` of `source` is made of, in the fragment's space, or `None` when one of
/// its subpaths is no accepted shape.
///
/// `tau_local` is the tolerance in the fragment's units. Recognition runs at the largest power of
/// two no larger than it, in the source's units, so one cached result serves every tolerance
/// within a factor of two and is never looser than asked.
pub(crate) fn recognised(
    source: &ShapeSource<'_>,
    outline: Outline,
    part: PartOf,
    tau_local: f64,
    max_prims: usize,
    masks: &dyn VectorMaskSource,
) -> Option<Arc<Decomposition>> {
    let (found, place) = recognised_in_source(source, outline, part, tau_local, max_prims, masks)?;
    Some(if place == Affine::IDENTITY {
        found
    } else {
        let [scale, _, _, _, x, y] = place.as_coeffs();
        Arc::new(moved(&found, scale, Vec2::new(x, y)))
    })
}

/// What [`recognised`] finds, in the units it was recognised in, and the matrix that maps it to
/// the fragment's space.
///
/// The matrix is `translate(offset) * scale(s)` under a uniform fit, and the identity for a placed
/// path. The result is the one the recognition cache holds, so a drawing whose fit changes keeps
/// it.
pub(crate) fn recognised_in_source(
    source: &ShapeSource<'_>,
    outline: Outline,
    part: PartOf,
    tau_local: f64,
    max_prims: usize,
    masks: &dyn VectorMaskSource,
) -> Option<(Arc<Decomposition>, Affine)> {
    let (shape, path, scale, offset) = read(source, outline)?;
    let inherited;
    let part = match part {
        PartOf::Fill(rule) => Part::Fill(rule),
        PartOf::OwnStroke => Part::Stroke(&shape.stroke.as_ref()?.style),
        PartOf::Inherited(width) => {
            inherited = kurbo::Stroke::new(width / scale);
            Part::Stroke(&inherited)
        }
    };
    let tau = tau_local / scale;
    if !tau.is_finite() || tau <= 0.0 {
        return None;
    }
    let class = tau.log2().floor() as i32;
    let key = match part {
        Part::Fill(_) => PartKey::Fill,
        Part::Stroke(style) => PartKey::stroke(style),
    };
    // The cache is not borrowed while recognition runs: the lookup and the insert each take it on
    // their own.
    let held = masks
        .recognitions()
        .and_then(|mut cache| cache.lookup(path, key, class, max_prims));
    let found = match held {
        Some(outcome) => outcome,
        None => {
            let limits = Limits {
                tau: 2.0_f64.powi(class),
                max_prims,
            };
            let (outcome, limit) = match recognise::recognise_or_decline(path, part, limits) {
                Ok(found) => (Some(Arc::new(found)), max_prims),
                Err(Declined::Limit) => (None, max_prims),
                // A subpath that is no shape is no shape at any limit.
                Err(Declined::Shape) => (None, usize::MAX),
            };
            if let Some(mut cache) = masks.recognitions() {
                cache.insert(path, key, class, limit, outcome.clone());
            }
            outcome
        }
    }?;
    Some((found, Affine::translate(offset) * Affine::scale(scale)))
}

/// `found`, scaled by `scale` and moved by `offset`.
fn moved(found: &Decomposition, scale: f64, offset: Vec2) -> Decomposition {
    let x = |value: f32| (f64::from(value) * scale + offset.x) as f32;
    let y = |value: f32| (f64::from(value) * scale + offset.y) as f32;
    let length = |value: f32| (f64::from(value) * scale) as f32;
    Decomposition {
        discs: found
            .discs
            .iter()
            .map(|&[cx, cy, outer, inner]| [x(cx), y(cy), length(outer), length(inner)])
            .collect(),
        boxes: found
            .boxes
            .iter()
            .map(|prim| recognise::BoxPrim {
                rect: [
                    x(prim.rect[0]),
                    y(prim.rect[1]),
                    x(prim.rect[2]),
                    y(prim.rect[3]),
                ],
                radii: prim.radii.map(length),
                exponent: prim.exponent,
                border: length(prim.border),
            })
            .collect(),
        capsules: found
            .capsules
            .iter()
            .map(|&[x0, y0, x1, y1]| [x(x0), y(y0), x(x1), y(y1)])
            .collect(),
        caps: found.caps.clone(),
        half_width: length(found.half_width),
        ink: [
            x(found.ink[0]),
            y(found.ink[1]),
            x(found.ink[2]),
            y(found.ink[3]),
        ],
        orientation: found.orientation,
        count: found.count,
        max_extent: length(found.max_extent),
    }
}

/// Whether every line of `path`, closing lines included, runs along an axis within a quarter of
/// `tau`, and no segment is a quadratic.
///
/// No quad draws a slanted edge, so a polygon or a slanted polyline fails here in one pass over
/// its elements, before recognition allocates anything.
pub(crate) fn straight_on_axes(path: &BezPath, tau: f64) -> bool {
    let off = tau / 4.0;
    let axial = |from: kurbo::Point, to: kurbo::Point| {
        let step = to - from;
        step.x.abs() <= off || step.y.abs() <= off
    };
    let mut start = kurbo::Point::ZERO;
    let mut at = kurbo::Point::ZERO;
    for element in path.elements() {
        match *element {
            kurbo::PathEl::MoveTo(point) => {
                start = point;
                at = point;
            }
            kurbo::PathEl::LineTo(point) => {
                if !axial(at, point) {
                    return false;
                }
                at = point;
            }
            kurbo::PathEl::CurveTo(_, _, point) => at = point,
            kurbo::PathEl::QuadTo(..) => return false,
            kurbo::PathEl::ClosePath => {
                if !axial(at, start) {
                    return false;
                }
                at = start;
            }
        }
    }
    true
}

/// `paint`, placed by the source's fit. A colour does not move; a ramp does.
pub(crate) fn placed_paint(source: &ShapeSource<'_>, paint: &zgui_svg::Paint) -> zgui_svg::Paint {
    if source.fit == Affine::IDENTITY {
        return paint.clone();
    }
    zgui_svg::document::place::paint(paint, source.fit)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zgui_scene::kurbo::{self, Affine, BezPath, Shape as _};
    use zgui_scene::peniko;

    use super::{Outline, PartOf, recognised};
    use crate::content::Drawing;
    use crate::content::vectors::NoVectorMasks;
    use crate::emit::vector::ShapeSource;
    use crate::emit::vector::recognise::{self, Decomposition, Limits, Part};

    /// Two circles, a rounded rectangle and a round-capped polyline, filled and stroked.
    fn shape() -> zgui_svg::Shape {
        let mut path = kurbo::Circle::new((10.0, 10.0), 4.0).to_path(0.01);
        path.extend(kurbo::Circle::new((13.0, 11.0), 3.0).path_elements(0.01));
        path.extend(
            kurbo::RoundedRect::new(20.0, 4.0, 32.0, 12.0, 2.0)
                .to_path(0.01)
                .elements()
                .iter()
                .copied(),
        );
        zgui_svg::Shape {
            path: Arc::new(path),
            fill: Some(zgui_svg::Fill {
                paint: zgui_svg::Paint::Solid(zgui_svg::Ink::Inherited { alpha: 1.0 }),
                rule: peniko::Fill::NonZero,
            }),
            stroke: Some(zgui_svg::Stroke {
                paint: zgui_svg::Paint::Solid(zgui_svg::Ink::Inherited { alpha: 1.0 }),
                style: kurbo::Stroke::new(1.0),
            }),
            clips: Vec::new(),
        }
    }

    /// Whether two decompositions agree to within a thousandth of a unit.
    fn agree(one: &Decomposition, two: &Decomposition) {
        let close = |a: &[f32], b: &[f32]| {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| (a - b).abs() <= 1.0e-3)
        };
        assert_eq!(one.count, two.count);
        assert!(close(one.discs.as_flattened(), two.discs.as_flattened()));
        for (a, b) in one.boxes.iter().zip(&two.boxes) {
            assert!(close(&a.rect, &b.rect) && close(&a.radii, &b.radii));
            assert!((a.border - b.border).abs() <= 1.0e-3);
        }
        assert!(close(&one.ink, &two.ink));
        assert!((one.half_width - two.half_width).abs() <= 1.0e-3);
    }

    #[test]
    fn recognition_through_a_uniform_fit_equals_recognition_of_the_placed_path() {
        let fit = Affine::translate((10.5, 7.25)) * Affine::scale(2.0);
        let drawing = Drawing::fitted(vec![shape()], fit);
        let source = ShapeSource::of(&drawing, 0);
        let tau = 1.0 / 16.0;
        let placed: BezPath = fit * shape().path.as_ref().clone();
        let limits = Limits {
            tau,
            max_prims: 256,
        };
        for (part, style) in [
            (PartOf::Fill(peniko::Fill::NonZero), None),
            (PartOf::OwnStroke, Some(kurbo::Stroke::new(2.0))),
        ] {
            let through = recognised(&source, Outline::Path, part, tau, 256, &NoVectorMasks)
                .expect("recognised through the fit");
            let expected = match &style {
                Some(style) => recognise::recognise(&placed, Part::Stroke(style), limits),
                None => recognise::recognise(&placed, Part::Fill(peniko::Fill::NonZero), limits),
            }
            .expect("recognised placed");
            agree(&through, &expected);
        }
        assert!(
            drawing.cell(0).and_then(|cell| cell.get()).is_none(),
            "a uniform fit recognises the source and places nothing"
        );
    }
}
