//! Flattening one parsed tree into the list of outlines a document draws.

use std::sync::Arc;

use crate::document::Unsupported;
use crate::document::place::uniform_scale;
use crate::document::shape::{Clip, Fill, Shape, Stroke};
use crate::parse::{clip, geometry, paint, stroke};

/// What is true of every shape below a given point in the tree.
#[derive(Clone, Debug, Default)]
struct Inside {
    /// Everything the groups above have folded into their children's alpha.
    opacity: f32,
    /// Every clip the groups above apply, which apply together.
    clips: Vec<Clip>,
}

/// The state of one flattening.
struct Flatten {
    /// The shapes, in painting order.
    shapes: Vec<Shape>,
    /// What was asked for that the model does not carry.
    unsupported: Unsupported,
}

/// Every outline `tree` draws, and what it asked for that this model cannot say.
pub(crate) fn document(tree: &usvg::Tree) -> (Vec<Shape>, Unsupported) {
    let mut flatten = Flatten {
        shapes: Vec::new(),
        unsupported: Unsupported::default(),
    };
    flatten.group(
        tree.root(),
        &Inside {
            opacity: 1.0,
            clips: Vec::new(),
        },
    );
    (flatten.shapes, flatten.unsupported)
}

impl Flatten {
    /// Appends everything one group draws.
    fn group(&mut self, group: &usvg::Group, inside: &Inside) {
        for child in group.children() {
            match child {
                usvg::Node::Group(nested) => {
                    let placement = geometry::affine(nested.abs_transform());
                    let mut clips = inside.clips.clone();
                    clips.extend(clip::of(nested, placement));
                    self.count(nested);
                    self.group(
                        nested,
                        &Inside {
                            opacity: inside.opacity * nested.opacity().get(),
                            clips,
                        },
                    );
                }
                usvg::Node::Path(path) => self.path(path, inside),
                usvg::Node::Image(_) => self.unsupported.images += 1,
                // Counted off the source instead: this crate reads with the parser's own text
                // support switched off, so a text element never reaches here at all.
                usvg::Node::Text(_) => {}
            }
        }
    }

    /// Records what one group asked for that a flat list of painted outlines cannot express.
    ///
    /// Every one of these draws *something* rather than nothing — the group's contents, without
    /// the effect. Dropping the whole group instead would turn a logo with one blurred shadow into
    /// no logo at all, and that is a worse answer than a logo with no shadow.
    fn count(&mut self, group: &usvg::Group) {
        if group.mask().is_some() {
            self.unsupported.masks += 1;
        }
        if !group.filters().is_empty() {
            self.unsupported.filters += 1;
        }
        if group.blend_mode() != usvg::BlendMode::Normal {
            self.unsupported.blend_modes += 1;
        }
    }

    /// Appends one outline, with its paint and its clips resolved.
    fn path(&mut self, path: &usvg::Path, inside: &Inside) {
        if !path.is_visible() {
            return;
        }
        let placement = geometry::affine(path.abs_transform());
        let fill = path.fill().and_then(|fill| {
            Some(Fill {
                paint: paint::of(
                    fill.paint(),
                    inside.opacity * fill.opacity().get(),
                    placement,
                    &mut self.unsupported,
                )?,
                rule: match fill.rule() {
                    usvg::FillRule::NonZero => peniko::Fill::NonZero,
                    usvg::FillRule::EvenOdd => peniko::Fill::EvenOdd,
                },
            })
        });
        let stroke = path.stroke().and_then(|source| {
            Some(Stroke {
                paint: paint::of(
                    source.paint(),
                    inside.opacity * source.opacity().get(),
                    placement,
                    &mut self.unsupported,
                )?,
                style: stroke::style(source, uniform_scale(placement)),
            })
        });
        if fill.is_none() && stroke.is_none() {
            return;
        }
        self.shapes.push(Shape {
            path: Arc::new(geometry::path(path.data(), placement)),
            fill,
            stroke,
            clips: inside.clips.clone(),
        });
    }
}
