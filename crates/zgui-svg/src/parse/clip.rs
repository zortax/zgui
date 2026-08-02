//! The regions a group keeps its content inside.

use std::sync::Arc;

use crate::document::shape::Clip;
use crate::parse::geometry;

/// Every clip one group applies, outermost first, in the document's own coordinates.
///
/// A group carrying no clip contributes none. A clip that itself references another contributes
/// both, because clipping a clip is an intersection and this list is an intersection.
///
/// `placement` is the group's own transform composed with everything above it: `clip-path` is
/// applied in the user space of the element that carries it, so a clipped group that is also
/// rotated has its clip rotated with it.
pub(crate) fn of(group: &usvg::Group, placement: kurbo::Affine) -> Vec<Clip> {
    let mut clips = Vec::new();
    let mut cursor = group.clip_path();
    while let Some(clip) = cursor {
        if let Some(shape) = outline(clip, placement) {
            clips.push(shape);
        }
        cursor = clip.clip_path();
    }
    clips
}

/// One clip path's whole outline.
///
/// A `clipPath` with several children keeps everything inside *any* of them — a union, not an
/// intersection — so the children become subpaths of one outline filled by the non-zero rule,
/// which is what a union of overlapping regions is. A child written with the even-odd rule is
/// folded into the same outline: the rule belongs to the whole clip here, and the alternative,
/// one clip layer per child, would be an intersection and would delete everything the children
/// did not all cover.
fn outline(clip: &usvg::ClipPath, placement: kurbo::Affine) -> Option<Clip> {
    let root = placement * geometry::affine(clip.transform());
    let mut path = kurbo::BezPath::new();
    let mut rule = None;
    collect(clip.root(), root, &mut path, &mut rule);
    (!path.is_empty()).then(|| Clip {
        path: Arc::new(path),
        rule: rule.unwrap_or(peniko::Fill::NonZero),
    })
}

/// Appends every outline of a clip path's tree.
fn collect(
    group: &usvg::Group,
    placement: kurbo::Affine,
    into: &mut kurbo::BezPath,
    rule: &mut Option<peniko::Fill>,
) {
    for child in group.children() {
        match child {
            usvg::Node::Group(nested) => collect(nested, placement, into, rule),
            usvg::Node::Path(shape) => {
                let placed = placement * geometry::affine(shape.abs_transform());
                into.extend(geometry::path(shape.data(), placed));
                // Only a clip with exactly one outline can honour an even-odd rule, because a
                // union of several written under one rule is not what either rule means. The
                // first child's rule is therefore kept and a second child forces the union rule.
                let child_rule =
                    shape
                        .fill()
                        .map_or(peniko::Fill::NonZero, |fill| match fill.rule() {
                            usvg::FillRule::NonZero => peniko::Fill::NonZero,
                            usvg::FillRule::EvenOdd => peniko::Fill::EvenOdd,
                        });
                *rule = Some(match rule {
                    None => child_rule,
                    Some(_) => peniko::Fill::NonZero,
                });
            }
            usvg::Node::Image(_) | usvg::Node::Text(_) => {}
        }
    }
}
