//! A colour a document names, which is either one colour or the one it is given.

use zgui_color::Color;

/// One colour of a resolved document.
///
/// The two arms are the whole colour rule of this crate. A document that wrote `currentColor`
/// produces [`Ink::Inherited`] and takes the colour of whatever element draws it, so one asset is
/// re-coloured by its context with nothing re-parsed. A document that wrote a colour produces
/// [`Ink::Solid`] and keeps it, so a multi-colour logo is never silently tinted by the text colour
/// of the paragraph it happens to sit in.
///
/// Both carry the alpha the document asked for. `fill-opacity`, `stroke-opacity`, `stop-opacity`
/// and the opacity of every group above them are folded into it as they are walked, because a
/// group that is half transparent is half transparent whichever of the two arms its children took.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ink {
    /// The colour the drawing element inherits, scaled by this alpha.
    Inherited {
        /// What multiplies the inherited colour's own alpha.
        alpha: f32,
    },
    /// This colour, whatever the element around it is.
    Solid(Color),
}

impl Ink {
    /// The colour this is, given the colour the element inherits.
    ///
    /// ```
    /// use zgui_color::Color;
    /// use zgui_svg::Ink;
    ///
    /// let inherited = Ink::Inherited { alpha: 0.5 };
    /// assert_eq!(inherited.resolve(Color::WHITE).components(), [1.0, 1.0, 1.0]);
    /// assert_eq!(inherited.resolve(Color::WHITE).alpha(), 0.5);
    ///
    /// // A document's own colour is not the element's, at any alpha.
    /// let own = Ink::Solid(Color::srgb(1.0, 0.0, 0.0, 1.0));
    /// assert_eq!(own.resolve(Color::WHITE).components(), [1.0, 0.0, 0.0]);
    /// ```
    pub fn resolve(self, inherited: Color) -> Color {
        match self {
            Self::Inherited { alpha } => inherited.with_alpha(inherited.alpha() * alpha),
            Self::Solid(color) => color,
        }
    }

    /// The same ink, at `factor` of the alpha.
    pub fn faded(self, factor: f32) -> Self {
        match self {
            Self::Inherited { alpha } => Self::Inherited {
                alpha: alpha * factor,
            },
            Self::Solid(color) => Self::Solid(color.with_alpha(color.alpha() * factor)),
        }
    }

    /// Whether this takes its colour from the element that draws it.
    pub fn is_inherited(self) -> bool {
        matches!(self, Self::Inherited { .. })
    }
}

#[cfg(test)]
mod tests {
    use zgui_color::Color;

    use super::Ink;

    #[test]
    fn fading_an_inherited_ink_does_not_turn_it_into_a_colour() {
        let faded = Ink::Inherited { alpha: 1.0 }.faded(0.25);
        assert!(faded.is_inherited());
        assert_eq!(faded.resolve(Color::BLACK).alpha(), 0.25);
    }

    #[test]
    fn fading_compounds_rather_than_replacing() {
        let ink = Ink::Solid(Color::srgb(1.0, 0.0, 0.0, 0.8)).faded(0.5);
        let Ink::Solid(color) = ink else {
            panic!("a colour stays a colour");
        };
        assert!((color.alpha() - 0.4).abs() < 1.0e-6, "{}", color.alpha());
    }
}
