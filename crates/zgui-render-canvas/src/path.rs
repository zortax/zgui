//! Paths and transforms shared by the Canvas primitive readers.

use wasm_bindgen::JsValue;
use web_sys::{CanvasRenderingContext2d, CanvasWindingRule, Path2d};
use zgui_geom::{Corners, DevicePx, Matrix4, Point, Rect, Size, Vec2};

pub(crate) fn from_bez(path: &kurbo::BezPath) -> Result<Path2d, JsValue> {
    let out = Path2d::new()?;
    append(&out, path);
    Ok(out)
}

pub(crate) fn append(out: &Path2d, path: &kurbo::BezPath) {
    for element in path.elements() {
        match *element {
            kurbo::PathEl::MoveTo(point) => out.move_to(point.x, point.y),
            kurbo::PathEl::LineTo(point) => out.line_to(point.x, point.y),
            kurbo::PathEl::QuadTo(control, point) => {
                out.quadratic_curve_to(control.x, control.y, point.x, point.y)
            }
            kurbo::PathEl::CurveTo(first, second, point) => {
                out.bezier_curve_to(first.x, first.y, second.x, second.y, point.x, point.y)
            }
            kurbo::PathEl::ClosePath => out.close_path(),
        }
    }
}

pub(crate) fn rounded(bounds: [f32; 4], radii: [f32; 8]) -> kurbo::BezPath {
    let [x, y, width, height] = bounds;
    zgui_scene::clip::path::rounded_rect(
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width.max(0.0)), DevicePx(height.max(0.0))),
        ),
        Corners {
            top_left: Vec2::new(DevicePx(radii[0]), DevicePx(radii[1])),
            top_right: Vec2::new(DevicePx(radii[2]), DevicePx(radii[3])),
            bottom_right: Vec2::new(DevicePx(radii[4]), DevicePx(radii[5])),
            bottom_left: Vec2::new(DevicePx(radii[6]), DevicePx(radii[7])),
        },
    )
}

pub(crate) fn inset(bounds: [f32; 4], radii: [f32; 8], edges: [f32; 4]) -> ([f32; 4], [f32; 8]) {
    let [top, right, bottom, left] = edges;
    let [x, y, width, height] = bounds;
    (
        [
            x + left,
            y + top,
            (width - left - right).max(0.0),
            (height - top - bottom).max(0.0),
        ],
        [
            (radii[0] - left).max(0.0),
            (radii[1] - top).max(0.0),
            (radii[2] - right).max(0.0),
            (radii[3] - top).max(0.0),
            (radii[4] - right).max(0.0),
            (radii[5] - bottom).max(0.0),
            (radii[6] - left).max(0.0),
            (radii[7] - bottom).max(0.0),
        ],
    )
}

pub(crate) fn matrix(context: &CanvasRenderingContext2d, matrix: Matrix4) -> Result<(), JsValue> {
    let c = matrix.columns;
    context.set_transform(
        c[0][0] as f64,
        c[0][1] as f64,
        c[1][0] as f64,
        c[1][1] as f64,
        c[3][0] as f64,
        c[3][1] as f64,
    )
}

pub(crate) fn affine(
    context: &CanvasRenderingContext2d,
    transform: kurbo::Affine,
) -> Result<(), JsValue> {
    let [a, b, c, d, e, f] = transform.as_coeffs();
    context.transform(a, b, c, d, e, f)
}

pub(crate) fn rule(fill: peniko::Fill) -> CanvasWindingRule {
    match fill {
        peniko::Fill::NonZero => CanvasWindingRule::Nonzero,
        peniko::Fill::EvenOdd => CanvasWindingRule::Evenodd,
    }
}
