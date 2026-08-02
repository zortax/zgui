//! `font-variant-numeric`.

/// Which figure shapes a face is asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NumericFigures {
    /// Not asked for: the face's default figures.
    #[default]
    Auto,
    /// `lining-nums` — figures that sit on the baseline at cap height.
    Lining,
    /// `oldstyle-nums` — figures with ascenders and descenders.
    Oldstyle,
}

/// How much room each figure takes.
///
/// The one setting in the whole `font-variant-numeric` family that reliably changes advances rather
/// than only outlines: tabular figures are all one width by construction, so a column of numbers
/// laid out with them is a different width from the same column laid out proportionally.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NumericSpacing {
    /// Not asked for: the face's default spacing.
    #[default]
    Auto,
    /// `proportional-nums` — each figure as wide as it needs to be.
    Proportional,
    /// `tabular-nums` — every figure the same width, so columns line up.
    Tabular,
}

/// Which fraction forms a face is asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NumericFractions {
    /// Not asked for.
    #[default]
    Auto,
    /// `diagonal-fractions` — numerator and denominator either side of a slash.
    Diagonal,
    /// `stacked-fractions` — numerator above denominator.
    Stacked,
}

/// `font-variant-numeric`, split into the four independent choices the grammar allows at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontVariantNumeric {
    /// The figure shapes.
    pub figures: NumericFigures,
    /// The figure spacing.
    pub spacing: NumericSpacing,
    /// The fraction forms.
    pub fractions: NumericFractions,
    /// `ordinal` — the letterforms that follow an ordinal number, as in `1st`.
    pub ordinal: bool,
    /// `slashed-zero` — a zero that cannot be read as a capital O.
    pub slashed_zero: bool,
}

impl FontVariantNumeric {
    /// `normal`: nothing asked for.
    pub const NORMAL: Self = Self {
        figures: NumericFigures::Auto,
        spacing: NumericSpacing::Auto,
        fractions: NumericFractions::Auto,
        ordinal: false,
        slashed_zero: false,
    };
}
