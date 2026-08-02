//! Backgrounds, list markers and anything else painted from an image or a ramp.

/// A position along a conic gradient's sweep: an angle, or a fraction of the whole turn.
pub use style::values::computed::AngleOrPercentage as AngleOrPercentageValue;
/// The computed value of `background-repeat`.
pub use style::values::computed::BackgroundRepeat as BackgroundRepeatValue;
/// The computed value of `background-size`.
pub use style::values::computed::BackgroundSize as BackgroundSizeValue;
/// A gradient, with its stops resolved and its interpolation space named.
///
/// A gradient's ramp is not sRGB unless it says so: the interpolation space travels with the value
/// so that a ramp asked for in Oklab is drawn in Oklab, rather than banded through sRGB on the way.
pub use style::values::computed::Gradient as GradientValue;
/// The computed value of `background-image` and every other image-valued property.
///
/// An image is a URL, a gradient, a cross-fade or an `image-set()` — the last of which has already
/// picked its entry, because the choice depends on the device pixel ratio and that is a cascade
/// input.
pub use style::values::computed::Image as ImageValue;
/// Which way a linear gradient's line points: an angle, an edge keyword, or a corner.
pub use style::values::computed::image::LineDirection as LineDirectionValue;
/// The computed value of `background-position-x`.
pub use style::values::computed::position::HorizontalPosition as HorizontalPositionValue;
/// The computed value of `background-position-y`.
pub use style::values::computed::position::VerticalPosition as VerticalPositionValue;
/// The state flags a gradient carries, of which `REPEATING` is the one that changes what is drawn.
pub use style::values::generics::image::GradientFlags;
/// One entry of a gradient's stop list: a stop, a positioned stop, or an interpolation hint.
pub use style::values::generics::image::GradientItem as GradientItemValue;
/// The horizontal half of a corner keyword.
pub use style::values::specified::position::HorizontalPositionKeyword;
/// The vertical half of a corner keyword.
pub use style::values::specified::position::VerticalPositionKeyword;
