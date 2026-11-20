//! A frame of the compositor, read off its transcript and its vector plan.

use zgui::geom::{Device, DevicePx, Rect};
use zgui_testkit_compositor as testkit;

use crate::device::frame::{Drawing, Filled, Frame, Glyph, Kind, rect_of};
use crate::device::picture::Picture;

/// An atlas tile a glyph sprite reads its coverage from.
///
/// Within one frame, sprites of the same glyph in the same style read the same tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    /// The atlas page.
    page: u32,
    /// The texel rectangle on the page.
    rect: [u32; 4],
}

impl Tile {
    /// Reads the `tile=page0[x, y, width, height]` field of a sprite.
    ///
    /// # Panics
    ///
    /// Panics when the sprite has no tile in that form.
    fn of(sprite: &testkit::Instance) -> Self {
        let parsed = sprite.field("tile").and_then(|field| {
            let (page, rect) = field.strip_prefix("page")?.split_once('[')?;
            let numbers: Vec<u32> = rect
                .strip_suffix(']')?
                .split(',')
                .map(|number| number.trim().parse().ok())
                .collect::<Option<_>>()?;
            Some(Self {
                page: page.parse().ok()?,
                rect: numbers.try_into().ok()?,
            })
        });
        parsed.unwrap_or_else(|| panic!("no tile in `{}`", sprite.text()))
    }
}

impl Frame {
    /// Reads the instances `drawing` drew and the vector passes `frame` planned, beside the
    /// composed target `pixels`.
    pub fn composed(
        frame: &compositor_types::Frame,
        drawing: &testkit::Drawing,
        pixels: &testkit::Pixels,
    ) -> Self {
        let quads: Vec<Filled> = drawing
            .quads()
            .into_iter()
            .enumerate()
            .map(|(order, quad)| Filled {
                order,
                bounds: placed(quad),
            })
            .collect();
        let glyphs = drawing
            .glyphs()
            .into_iter()
            .map(|sprite| Glyph {
                bounds: placed(sprite),
                tile: Tile::of(sprite),
            })
            .collect();

        let mut drawings: Vec<Drawing> = drawing
            .of_kind("path")
            .enumerate()
            .map(|(order, path)| Drawing {
                order,
                kind: Kind::Vector,
                ink: placed_ink(path),
            })
            .collect();
        for (kind, name) in [(Kind::Colour, "color"), (Kind::Mark, "marks")] {
            drawings.extend(
                drawing
                    .of_kind(name)
                    .enumerate()
                    .map(|(order, item)| Drawing {
                        order,
                        kind,
                        ink: placed(item),
                    }),
            );
        }
        let mut shapes = drawings.clone();
        shapes.extend(quads.iter().map(|quad| Drawing {
            order: quad.order,
            kind: Kind::Quad,
            ink: quad.bounds,
        }));
        shapes.extend(
            drawing
                .of_kind("mono")
                .enumerate()
                .map(|(order, sprite)| Drawing {
                    order,
                    kind: Kind::Mask,
                    ink: placed(sprite),
                }),
        );

        let picture = Picture::of_compositor(pixels);
        Self {
            blank: drawing.instances().is_empty(),
            pixels: picture.clone(),
            composed: picture,
            drawings,
            shapes,
            quads,
            glyphs,
            pass_regions: frame
                .vector_plan
                .passes
                .iter()
                .map(|pass| {
                    let region = pass.region;
                    let region = Rect::new(
                        zgui::geom::Point::new(region.origin.x, region.origin.y),
                        zgui::geom::Size::new(region.size.width, region.size.height),
                    );
                    let items = pass.items.start as usize..pass.items.end as usize;
                    (region, items, pass.instanced)
                })
                .collect(),
        }
    }
}

/// Where `instance`'s bounds land once its world matrix has been applied.
fn placed(instance: &testkit::Instance) -> Rect<DevicePx, Device> {
    placed_rect(instance, instance.bounds())
}

/// Where `rect`, in `instance`'s own space, lands once its world matrix has been applied.
///
/// The box around the four transformed corners is taken, so a rotation answers with the rectangle
/// its ink covers.
fn placed_rect(instance: &testkit::Instance, rect: [f32; 4]) -> Rect<DevicePx, Device> {
    let [x, y, width, height] = rect;
    let Some(matrix) = instance
        .field("space")
        .map(|space| matrix_of(instance, space))
    else {
        return rect_of([x, y, width, height]);
    };

    let corners = [
        (x, y),
        (x + width, y),
        (x, y + height),
        (x + width, y + height),
    ]
    .map(|(x, y)| apply(&matrix, x, y));
    let left = corners.iter().map(|point| point.0).fold(f32::MAX, f32::min);
    let right = corners.iter().map(|point| point.0).fold(f32::MIN, f32::max);
    let top = corners.iter().map(|point| point.1).fold(f32::MAX, f32::min);
    let bottom = corners.iter().map(|point| point.1).fold(f32::MIN, f32::max);
    rect_of([left, top, right - left, bottom - top])
}

/// Where a path's `ink=` rectangle lands once its world matrix has been applied.
///
/// # Panics
///
/// Panics when the path has no ink rectangle.
fn placed_ink(path: &testkit::Instance) -> Rect<DevicePx, Device> {
    let ink = path
        .rect("ink")
        .unwrap_or_else(|| panic!("no ink in `{}`", path.text()));
    placed_rect(path, ink)
}

/// Reads a `space=` field, `translate[x, y]` or four columns `[[..], [..], [..], [..]]`, as
/// columns.
///
/// # Panics
///
/// Panics when the field is in neither form.
fn matrix_of(instance: &testkit::Instance, space: &str) -> [[f32; 4]; 4] {
    let numbers: Vec<f32> = space
        .trim_start_matches("translate")
        .split(['[', ']', ','])
        .map(str::trim)
        .filter(|number| !number.is_empty())
        .map(|number| number.parse().ok())
        .collect::<Option<_>>()
        .unwrap_or_else(|| panic!("no matrix in `{}`", instance.text()));
    match numbers[..] {
        [x, y] => [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [x, y, 0.0, 1.0],
        ],
        _ => {
            let columns: [f32; 16] = numbers
                .try_into()
                .unwrap_or_else(|_| panic!("no matrix in `{}`", instance.text()));
            core::array::from_fn(|column| core::array::from_fn(|row| columns[column * 4 + row]))
        }
    }
}

/// Puts the point `(x, y)` through `matrix`, with the perspective divide.
fn apply(matrix: &[[f32; 4]; 4], x: f32, y: f32) -> (f32, f32) {
    let row = |at: usize| matrix[0][at] * x + matrix[1][at] * y + matrix[3][at];
    let w = row(3);
    (row(0) / w, row(1) / w)
}
