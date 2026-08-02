//! Adding a CSS-space position to a device-space position must not compile.

use zgui_geom::{Css, CssPx, Device, Point};

fn main() {
    let css: Point<CssPx, Css> = Point::new(CssPx(1.0), CssPx(2.0));
    let device: Point<CssPx, Device> = Point::new(CssPx(3.0), CssPx(4.0));
    let _ = css + device;
}
