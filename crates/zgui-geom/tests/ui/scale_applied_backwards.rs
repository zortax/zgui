//! Applying a device-to-CSS scale to CSS geometry must not compile.

use zgui_geom::{Css, CssPx, Device, Point, Scale};

fn main() {
    let css: Point<CssPx, Css> = Point::new(CssPx(1.0), CssPx(2.0));
    let to_css: Scale<Device, Css> = Scale::new(0.5);
    let _ = css * to_css;
}
