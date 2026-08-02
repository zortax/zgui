//! Building a rectangle from corners in two different spaces must not compile.

use zgui_geom::{Css, CssPx, Device, Point, Rect};

fn main() {
    let near: Point<CssPx, Css> = Point::new(CssPx(0.0), CssPx(0.0));
    let far: Point<CssPx, Device> = Point::new(CssPx(10.0), CssPx(10.0));
    let _: Rect<CssPx, Css> = Rect::from_corners(near, far);
}
