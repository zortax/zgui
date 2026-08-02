//! Read extents, and the two records that must agree about them.

use smallvec::smallvec;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};

use crate::group::{BackdropFilter, Filter, GroupBoundary, read_extent};

/// A device rectangle.
fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

#[test]
fn every_per_pixel_filter_reads_exactly_what_it_writes() {
    let bounds = rect(10.0, 10.0, 30.0, 20.0);
    let per_pixel = [
        Filter::Brightness(1.2),
        Filter::Contrast(0.8),
        Filter::Grayscale(1.0),
        Filter::HueRotate(1.0),
        Filter::Invert(1.0),
        Filter::Opacity(0.5),
        Filter::Saturate(2.0),
        Filter::Sepia(1.0),
    ];
    for filter in per_pixel {
        assert!(filter.is_per_pixel(), "{filter:?} should be per-pixel");
        assert_eq!(read_extent(bounds, &[filter]), bounds);
    }
    assert_eq!(read_extent(bounds, &per_pixel), bounds);
    assert_eq!(read_extent(bounds, &[]), bounds);
}

#[test]
fn a_drop_shadows_extent_leans_the_way_it_is_offset() {
    let bounds = rect(100.0, 100.0, 20.0, 20.0);
    let extent = read_extent(
        bounds,
        &[Filter::DropShadow {
            offset_x: 6.0,
            offset_y: 0.0,
            blur: 2.0,
            color: [0.0, 0.0, 0.0, 1.0],
        }],
    );
    // Six of the six-pixel blur reach is consumed by the offset on the left, and added on the right.
    assert_eq!(extent.origin.x, DevicePx(100.0));
    assert_eq!(extent.right(), DevicePx(132.0));
}

#[test]
fn a_chain_of_blurs_adds_rather_than_taking_the_largest() {
    let bounds = rect(0.0, 0.0, 10.0, 10.0);
    let one = read_extent(bounds, &[Filter::Blur(1.0)]);
    let two = read_extent(bounds, &[Filter::Blur(1.0), Filter::Blur(1.0)]);
    assert_eq!(one.size, Size::new(DevicePx(16.0), DevicePx(16.0)));
    assert_eq!(two.size, Size::new(DevicePx(22.0), DevicePx(22.0)));
}

/// The two records that carry a read extent must derive it the same way, because a damage
/// expansion evaluating one function and a cull rule evaluating another is exactly how a frosted
/// panel starts to smear.
#[test]
fn a_group_and_a_backdrop_agree_about_what_they_read() {
    let bounds = rect(40.0, 40.0, 80.0, 24.0);
    let filters = smallvec![Filter::Blur(6.0)];

    let group = GroupBoundary::start(bounds, 1.0, peniko::BlendMode::default(), filters.clone());
    let backdrop = BackdropFilter::new(bounds, filters.clone());

    assert_eq!(group.source, backdrop.source);
    assert_eq!(group.source, read_extent(bounds, &filters));
    assert!(!group.reads_only_what_it_writes());
    assert!(!backdrop.reads_only_what_it_writes());
}

#[test]
fn plain_opacity_and_a_blend_mode_read_nothing_extra() {
    let bounds = rect(0.0, 0.0, 10.0, 10.0);
    let group = GroupBoundary::start(
        bounds,
        0.5,
        peniko::BlendMode::new(peniko::Mix::Multiply, peniko::Compose::SrcOver),
        smallvec![],
    );
    assert!(group.reads_only_what_it_writes());
    assert_eq!(group.source, bounds);
}

#[test]
fn a_closing_marker_matches_its_opening_one_in_everything_but_direction() {
    let group = GroupBoundary::start(
        rect(0.0, 0.0, 10.0, 10.0),
        0.5,
        peniko::BlendMode::default(),
        smallvec![Filter::Blur(1.0)],
    );
    let end = group.end();
    assert!(group.is_start && !end.is_start);
    assert_eq!(group.bounds, end.bounds);
    assert_eq!(group.source, end.source);
    assert_eq!(group.filters, end.filters);
}
