//! A clip link's rectangle and radii, put where its coordinate system is drawn.

use zgui_geom::{Corners, Device, DevicePx, Matrix4, Point, Rect, Size, Vec2};

/// One link's rectangle and radii, put where its coordinate system's matrix says it is drawn.
///
/// The four corners go through the matrix and the upright box around them is taken, so a rotation
/// answers with the box its clip actually reaches rather than a rectangle that never turned. The
/// radii are scaled by the length each axis maps to, which is exact for every translation and
/// scale and a generous reading for a rotation — a clip slightly too admitting, never one that
/// cuts what the transform kept inside it.
pub(super) fn placed(
    rect: Rect<DevicePx, Device>,
    radii: Corners<Vec2<DevicePx>>,
    matrix: Option<Matrix4>,
) -> (Rect<DevicePx, Device>, Corners<Vec2<DevicePx>>) {
    let Some(matrix) = matrix else {
        return (rect, radii);
    };
    match matrix.plane_translation() {
        Some(by) => moved(rect, radii, by),
        None => mapped(rect, radii, &matrix),
    }
}

/// `rect` and `radii` moved by `by`, with the same arithmetic [`mapped`] does for a translation.
fn moved(
    rect: Rect<DevicePx, Device>,
    radii: Corners<Vec2<DevicePx>>,
    (tx, ty): (f32, f32),
) -> (Rect<DevicePx, Device>, Corners<Vec2<DevicePx>>) {
    let (x, y) = (rect.origin.x.0, rect.origin.y.0);
    let (near_x, far_x) = (x + tx, (x + rect.size.width.0) + tx);
    let (near_y, far_y) = (y + ty, (y + rect.size.height.0) + ty);
    let (left, right) = (near_x.min(far_x), near_x.max(far_x));
    let (top, bottom) = (near_y.min(far_y), near_y.max(far_y));
    let moved = Rect::new(
        Point::new(DevicePx(left), DevicePx(top)),
        Size::new(DevicePx(right - left), DevicePx(bottom - top)),
    );
    // A unit vector of each axis, measured between two translated points as `mapped` measures
    // it. Its other component is zero.
    let scale_x = ((1.0 + tx) - tx).powi(2).sqrt();
    let scale_y = ((1.0 + ty) - ty).powi(2).sqrt();
    (moved, scaled(radii, scale_x, scale_y))
}

/// `rect` and `radii` put through `matrix`.
fn mapped(
    rect: Rect<DevicePx, Device>,
    radii: Corners<Vec2<DevicePx>>,
    matrix: &Matrix4,
) -> (Rect<DevicePx, Device>, Corners<Vec2<DevicePx>>) {
    let (x, y) = (rect.origin.x.0, rect.origin.y.0);
    let (width, height) = (rect.size.width.0, rect.size.height.0);
    let corners = [
        matrix.transform_point(x, y, 0.0),
        matrix.transform_point(x + width, y, 0.0),
        matrix.transform_point(x, y + height, 0.0),
        matrix.transform_point(x + width, y + height, 0.0),
    ];
    let left = corners
        .iter()
        .map(|point| point[0])
        .fold(f32::MAX, f32::min);
    let right = corners
        .iter()
        .map(|point| point[0])
        .fold(f32::MIN, f32::max);
    let top = corners
        .iter()
        .map(|point| point[1])
        .fold(f32::MAX, f32::min);
    let bottom = corners
        .iter()
        .map(|point| point[1])
        .fold(f32::MIN, f32::max);
    let moved = Rect::new(
        Point::new(DevicePx(left), DevicePx(top)),
        Size::new(DevicePx(right - left), DevicePx(bottom - top)),
    );
    // How long one unit of each axis is once mapped, which is what an ellipse radius scales by.
    let origin = matrix.transform_point(0.0, 0.0, 0.0);
    let unit_x = matrix.transform_point(1.0, 0.0, 0.0);
    let unit_y = matrix.transform_point(0.0, 1.0, 0.0);
    let scale_x = ((unit_x[0] - origin[0]).powi(2) + (unit_x[1] - origin[1]).powi(2)).sqrt();
    let scale_y = ((unit_y[0] - origin[0]).powi(2) + (unit_y[1] - origin[1]).powi(2)).sqrt();
    (moved, scaled(radii, scale_x, scale_y))
}

/// `radii` with each radius scaled by the length its axis maps to.
fn scaled(radii: Corners<Vec2<DevicePx>>, scale_x: f32, scale_y: f32) -> Corners<Vec2<DevicePx>> {
    let scaled = |radius: Vec2<DevicePx>| {
        Vec2::new(
            DevicePx(radius.x.0 * scale_x),
            DevicePx(radius.y.0 * scale_y),
        )
    };
    Corners {
        top_left: scaled(radii.top_left),
        top_right: scaled(radii.top_right),
        bottom_right: scaled(radii.bottom_right),
        bottom_left: scaled(radii.bottom_left),
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Corners, DevicePx, Matrix4, Point, Rect, Size, Vec2};

    use super::{mapped, moved};

    #[test]
    fn a_translation_moves_a_link_as_the_matrix_maps_it() {
        let rect = Rect::new(
            Point::new(DevicePx(10.25), DevicePx(-3.5)),
            Size::new(DevicePx(100.125), DevicePx(40.0)),
        );
        let radii = Corners::uniform(Vec2::new(DevicePx(6.0), DevicePx(4.5)));
        for by in [
            (0.0, 0.0),
            (0.1, -7.3),
            (-1834.6, 2_097_151.5),
            (3.0e7, -1.0e8),
        ] {
            let matrix = Matrix4::translation(by.0, by.1, 0.0);
            assert_eq!(
                moved(rect, radii, by),
                mapped(rect, radii, &matrix),
                "{by:?}"
            );
        }
    }
}
