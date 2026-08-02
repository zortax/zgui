//! `writing-mode`.

/// The axis a paragraph's lines run along, and the direction they stack in.
///
/// Only the three values a style sheet can reach in this framework's build are here. `sideways-rl`
/// and `sideways-lr` are generated for another engine and the parser does not accept them, so a
/// variant for either would be a value nothing could ever produce.
///
/// This is a *shaping* property and not a breaking one, which is not obvious. Turning a paragraph
/// vertical does not merely stack the lines differently: a face's vertical substitutions replace
/// glyphs with rotated or repositioned forms, the vertical advance is read from a different table,
/// and characters that are upright in horizontal text are laid on their side. So the glyphs
/// themselves change, and a shaped result taken horizontally cannot be re-broken vertically.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WritingMode {
    /// `horizontal-tb` — lines run horizontally and stack downwards.
    #[default]
    HorizontalTb,
    /// `vertical-rl` — lines run vertically and stack right to left.
    VerticalRl,
    /// `vertical-lr` — lines run vertically and stack left to right.
    VerticalLr,
}

impl WritingMode {
    /// Whether lines run down the page rather than across it.
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::VerticalRl | Self::VerticalLr)
    }
}
