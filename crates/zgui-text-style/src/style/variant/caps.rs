//! `font-variant-caps`.

/// Which capital forms a face is asked for.
///
/// Only the two values a style sheet can actually reach are here. The remaining five keywords of
/// the CSS grammar — `all-small-caps`, `petite-caps`, `all-petite-caps`, `unicase` and
/// `titling-caps` — are not values the parser this framework is built on accepts, so a variant for
/// each would be a value nothing could ever produce.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FontVariantCaps {
    /// `normal` — the face's ordinary letterforms.
    #[default]
    Normal,
    /// `small-caps` — the face's small-capital forms, where it has them.
    SmallCaps,
}
