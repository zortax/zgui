//! What the parser says strokes a shape, as the stroke style a path renderer takes.

/// The stroke style `source` describes, at a geometry scaled by `scale`.
///
/// Every outline of a parsed document is placed by the transform of the groups above it, so a
/// stroke's width, its dash lengths and its dash offset are scaled by the same factor — a shape
/// inside `transform="scale(2)"` is drawn twice as large *and* twice as thick, which is what SVG
/// says and what a reader of the picture expects.
///
/// The factor is the square root of the matrix's determinant, which is the uniform scale of a
/// matrix that has one. A matrix that scales the two axes differently has no single stroke width,
/// and this takes the one that preserves the stroked area.
pub(crate) fn style(source: &usvg::Stroke, scale: f64) -> kurbo::Stroke {
    let mut style = kurbo::Stroke::new(f64::from(source.width().get()) * scale)
        .with_caps(cap(source.linecap()))
        .with_join(join(source.linejoin()))
        .with_miter_limit(f64::from(source.miterlimit().get()));
    if let Some(dashes) = source.dasharray() {
        style = style.with_dashes(
            f64::from(source.dashoffset()) * scale,
            dashes.iter().map(|dash| f64::from(*dash) * scale),
        );
    }
    style
}

/// The end of a stroke.
fn cap(source: usvg::LineCap) -> kurbo::Cap {
    match source {
        usvg::LineCap::Butt => kurbo::Cap::Butt,
        usvg::LineCap::Round => kurbo::Cap::Round,
        usvg::LineCap::Square => kurbo::Cap::Square,
    }
}

/// The corner between two segments of a stroke.
///
/// A clipped miter is a miter here. The difference between the two shows only past the miter
/// limit, where one falls back to a bevel and the other to a flat cut, and there is no third join
/// in the geometry vocabulary this framework draws through — so it is drawn as the join it is
/// closest to rather than as a bevel, which is what the fallback would otherwise be.
fn join(source: usvg::LineJoin) -> kurbo::Join {
    match source {
        usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => kurbo::Join::Miter,
        usvg::LineJoin::Round => kurbo::Join::Round,
        usvg::LineJoin::Bevel => kurbo::Join::Bevel,
    }
}

#[cfg(test)]
mod tests {
    use crate::document::place::uniform_scale as scale_of;

    #[test]
    fn a_uniform_scale_is_read_back_exactly() {
        assert!((scale_of(kurbo::Affine::scale(3.0)) - 3.0).abs() < 1.0e-9);
    }

    #[test]
    fn a_rotation_or_a_translation_does_not_thicken_a_stroke() {
        assert!((scale_of(kurbo::Affine::rotate(1.1)) - 1.0).abs() < 1.0e-9);
        assert!((scale_of(kurbo::Affine::translate((40.0, -3.0))) - 1.0).abs() < 1.0e-9);
    }
}
