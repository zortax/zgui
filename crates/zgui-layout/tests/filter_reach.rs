//! How far a filter chain reads outside the box it writes.

use zgui_css::shader::ShaderId;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_layout::fragment::filter::{Filter, read_extent};

fn bounds() -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(100.0), DevicePx(100.0)),
    )
}

fn effect(reach: f32) -> Filter {
    Filter::Custom {
        shader: ShaderId(1),
        reach,
    }
}

#[test]
fn an_effect_with_no_reach_is_per_pixel() {
    assert!(effect(0.0).is_per_pixel());
    assert_eq!(read_extent(bounds(), &[effect(0.0)]), bounds());
}

#[test]
fn an_effect_reads_its_reach_outside_the_box_on_every_side() {
    let read = read_extent(bounds(), &[effect(8.0)]);

    assert_eq!(read.origin, Point::new(DevicePx(-8.0), DevicePx(-8.0)));
    assert_eq!(read.size, Size::new(DevicePx(116.0), DevicePx(116.0)));
}

#[test]
fn a_reach_below_zero_reads_nothing_outside() {
    assert_eq!(effect(-4.0).kernel_support(), (0.0, 0.0, 0.0, 0.0));
}

#[test]
fn the_supports_of_a_chain_add_up() {
    let read = read_extent(bounds(), &[Filter::Blur(2.0), effect(4.0)]);

    assert_eq!(
        read.origin.x.0,
        -(Filter::BLUR_EXTENT * 2.0 + Filter::RESAMPLE_REACH + 4.0)
    );
}
