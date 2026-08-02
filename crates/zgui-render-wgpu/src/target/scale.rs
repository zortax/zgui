//! How many texels of a target one device pixel covers.

use zgui_geom::{Device, Size};

/// The resolution a target is held at, relative to the device pixel grid.
///
/// Two things need this and they are the same thing: a separable blur runs at half resolution
/// because that is four times less work for a difference no eye finds, and a target pool at its
/// memory budget degrades to half resolution rather than to no isolation at all — a blurrier
/// frosted panel rather than a panel composited in the wrong place.
///
/// It is an enumeration rather than a float because both users want exactly one reduction and the
/// halved extent has to be reproducible: a rounding rule applied twice to an arbitrary factor is
/// how a source rectangle and the target holding it end up one texel apart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TargetScale {
    /// One texel per device pixel.
    #[default]
    Full,
    /// One texel per two device pixels, on both axes.
    Half,
}

impl TargetScale {
    /// Both resolutions.
    pub const ALL: [Self; 2] = [Self::Full, Self::Half];

    /// How many texels one device pixel covers.
    pub fn factor(self) -> f32 {
        match self {
            Self::Full => 1.0,
            Self::Half => 0.5,
        }
    }

    /// The extent in texels a region of `size` device pixels needs.
    ///
    /// Rounded **up**, so a region of odd extent still has a texel for its last half pixel and the
    /// blur does not read past the end of what was written into.
    pub fn extent(self, size: Size<i32, Device>) -> Size<i32, Device> {
        match self {
            Self::Full => size.non_negative(),
            Self::Half => Size::new(half_up(size.width).max(1), half_up(size.height).max(1)),
        }
    }

    /// The texel a device pixel coordinate falls in, rounded towards zero.
    pub fn texel(self, pixels: i32) -> i32 {
        match self {
            Self::Full => pixels,
            Self::Half => pixels.div_euclid(2),
        }
    }
}

/// Half of `length`, rounded away from zero.
fn half_up(length: i32) -> i32 {
    (length.max(0) + 1) / 2
}

#[cfg(test)]
mod tests {
    use super::TargetScale;
    use zgui_geom::Size;

    #[test]
    fn halving_an_odd_extent_rounds_up_so_the_last_half_pixel_has_a_texel() {
        assert_eq!(TargetScale::Half.extent(Size::new(9, 1)), Size::new(5, 1));
        assert_eq!(TargetScale::Full.extent(Size::new(9, 1)), Size::new(9, 1));
    }

    #[test]
    fn an_empty_extent_still_allocates_a_texel_because_a_texture_cannot_be_empty() {
        assert_eq!(TargetScale::Half.extent(Size::new(0, 0)), Size::new(1, 1));
    }

    #[test]
    fn the_factor_and_the_extent_agree() {
        for scale in TargetScale::ALL {
            let extent = scale.extent(Size::new(64, 64));
            assert_eq!(extent.width as f32, 64.0 * scale.factor());
        }
    }
}
