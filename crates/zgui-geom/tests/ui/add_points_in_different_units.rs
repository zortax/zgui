//! Adding a position in CSS pixels to one in device pixels must not compile.

use zgui_geom::{Css, CssPx, DevicePx, Point};

fn main() {
    let css: Point<CssPx, Css> = Point::new(CssPx(1.0), CssPx(2.0));
    let device: Point<DevicePx, Css> = Point::new(DevicePx(3.0), DevicePx(4.0));
    let _ = css + device;
}
