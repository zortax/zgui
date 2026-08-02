//! Where the caret is, in the units the platform is told about.

use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Scale, Size};
use zgui_platform::{TextInput, TextInputPurpose};

/// What the platform is told about a caret sitting at `origin` and `height` tall.
///
/// The conversion is the whole of what this does, and it is the whole of what a caller gets wrong:
/// fragments and carets are in device pixels because that is what the renderer draws in, and the
/// window is told about text input in CSS pixels because that is what it positions its candidate
/// window in. A surface at twice the scale that was handed device pixels puts the candidate list
/// off the bottom of a tall field.
pub fn caret_area(
    origin: Point<DevicePx, Device>,
    height: DevicePx,
    scale: Scale<Css, Device>,
    purpose: TextInputPurpose,
) -> TextInput {
    let factor = scale.get().max(f32::MIN_POSITIVE);
    TextInput {
        caret_origin: Point::new(CssPx(origin.x.0 / factor), CssPx(origin.y.0 / factor)),
        caret_size: Size::<CssPx, Css>::new(CARET_WIDTH, CssPx(height.0 / factor)),
        purpose,
    }
}

/// How wide the caret is reported as being.
///
/// A candidate window is placed beside the caret rather than over it, so what matters is that the
/// width is not zero; one CSS pixel is what a text field draws.
const CARET_WIDTH: CssPx = CssPx(1.0);

#[cfg(test)]
mod tests {
    use zgui_geom::{Device, DevicePx, Point, Scale};
    use zgui_platform::TextInputPurpose;

    use super::caret_area;

    #[test]
    fn a_caret_on_a_doubled_surface_is_reported_in_css_pixels() {
        let area = caret_area(
            Point::<DevicePx, Device>::new(DevicePx(240.0), DevicePx(96.0)),
            DevicePx(32.0),
            Scale::new(2.0),
            TextInputPurpose::Normal,
        );
        assert_eq!(area.caret_origin.x.0, 120.0);
        assert_eq!(area.caret_origin.y.0, 48.0);
        assert_eq!(area.caret_size.height.0, 16.0);
        assert!(
            area.caret_size.width.0 > 0.0,
            "a candidate window needs a box"
        );
    }

    #[test]
    fn a_secret_field_says_so_where_the_input_method_can_see_it() {
        let area = caret_area(
            Point::<DevicePx, Device>::new(DevicePx(0.0), DevicePx(0.0)),
            DevicePx(16.0),
            Scale::new(1.0),
            TextInputPurpose::Password,
        );
        assert!(area.purpose.is_secret());
    }
}
