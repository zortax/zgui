//! The fingerprint of a fragment's resolved corner radii.
//!
//! The lowered style holds no radii, because a percentage radius resolves against the box that
//! carries it. A recorded range is encoded with the radii in force, so the replay test compares
//! them through this fingerprint.

use zgui_geom::{Corners, DevicePx, Vec2};

/// A fingerprint of `radii`, for deciding whether a recorded fragment may be replayed.
pub fn signature(radii: Corners<Vec2<DevicePx>>) -> u64 {
    let mut hash = zgui_space::ContentHash::new();
    for corner in [
        radii.top_left,
        radii.top_right,
        radii.bottom_right,
        radii.bottom_left,
    ] {
        hash = hash.f32(corner.x.0).f32(corner.y.0);
    }
    hash.finish()
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Corners, DevicePx, Vec2};

    use super::signature;

    /// Four equal corners of `size` device pixels.
    fn round(size: f32) -> Corners<Vec2<DevicePx>> {
        let corner = Vec2::new(DevicePx(size), DevicePx(size));
        Corners::new(corner, corner, corner, corner)
    }

    #[test]
    fn one_squared_corner_moves_the_fingerprint() {
        let mut squared = round(6.0);
        squared.bottom_left = Vec2::new(DevicePx(0.0), DevicePx(0.0));
        assert_ne!(signature(round(6.0)), signature(squared));
        assert_eq!(signature(round(6.0)), signature(round(6.0)));
    }
}
