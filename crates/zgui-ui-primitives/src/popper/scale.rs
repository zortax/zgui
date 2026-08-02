//! The two kinds of pixel a placement passes through, and the one conversion between them.
//!
//! A placement is made from measurements — the anchor's box, the surface's own size, the window's
//! rectangle — and every one of those is delivered in **device** pixels, because that is the space
//! the layout is resolved and the pointer is hit-tested in. It is written back as an inline
//! `left`/`top`, and a length in a style sheet is a **CSS** pixel. The two are the same number only
//! on a display of one device pixel per CSS pixel, which is the display nearly every fixture runs
//! on and the reason a confusion between them is invisible until somebody opens the window on a
//! denser output.
//!
//! What it looks like when they are confused: the surface lands at the anchor's coordinates
//! *multiplied* by the density, so a trigger a few pixels down the page opens its list a few pixels
//! out of place and a trigger several hundred pixels into a page opens it several hundred pixels
//! away — below and to the right of everything, in the wrong panel entirely.

/// How many device pixels one CSS pixel is on the surface a floating surface is being placed on.
///
/// Constructed from what the element reports rather than assumed, and never zero: a density of
/// nothing would divide a placement into infinity and put the surface nowhere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Density(f32);

/// The smallest density that can be divided by and still mean something.
const FLOOR: f32 = 0.01;

impl Density {
    /// The density an element reports, made safe to divide by.
    pub(crate) fn reported(scale: f32) -> Self {
        Self(if scale.is_finite() && scale > FLOOR {
            scale
        } else {
            1.0
        })
    }

    /// `css` CSS pixels as device pixels, which is the space a placement is solved in.
    pub(crate) fn device(self, css: f32) -> f32 {
        css * self.0
    }

    /// `device` device pixels as CSS pixels, which is the space an inline length is read in.
    pub(crate) fn css(self, device: f32) -> f32 {
        device / self.0
    }
}

#[cfg(test)]
mod tests {
    use super::Density;

    #[test]
    fn a_density_of_one_leaves_both_directions_alone() {
        let density = Density::reported(1.0);
        assert!((density.css(400.0) - 400.0).abs() < f32::EPSILON);
        assert!((density.device(4.0) - 4.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_denser_display_shortens_a_length_written_into_a_style() {
        // The whole defect, in one number: a surface solved at 500 device pixels on a display of
        // 1.25 is written as 400 CSS pixels, and lands back at 500.
        let density = Density::reported(1.25);
        assert!((density.css(500.0) - 400.0).abs() < 0.001);
        assert!((density.device(density.css(500.0)) - 500.0).abs() < 0.001);
    }

    #[test]
    fn a_density_that_cannot_be_divided_by_is_refused() {
        for reported in [0.0, -2.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                Density::reported(reported),
                Density::reported(1.0),
                "a placement divided by {reported} is a surface nobody can see"
            );
        }
    }
}
