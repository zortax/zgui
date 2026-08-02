//! Spacing, wrapping, alignment and white-space handling.

/// `direction`, the paragraph's base writing direction.
pub use style::computed_values::direction::T as Direction;
/// `text-wrap-mode`, which decides whether soft wrapping happens at all.
pub use style::computed_values::text_wrap_mode::T as TextWrapMode;
/// `white-space-collapse`, which decides what text the shaper is handed in the first place.
pub use style::computed_values::white_space_collapse::T as WhiteSpaceCollapse;
/// `writing-mode`, the axis lines are stacked along.
///
/// This build offers the three horizontal-and-vertical keywords only: `sideways-rl` and
/// `sideways-lr` are generated for another engine and the parser here does not accept them.
pub use style::computed_values::writing_mode::T as WritingModeProperty;
/// `letter-spacing`, extra advance after every cluster.
pub use style::values::computed::LetterSpacing;
/// `line-break`, the strictness of the break opportunities a line may be cut at.
pub use style::values::computed::LineBreak;
/// `overflow-wrap`, emergency breaking inside a word that does not fit on its own line.
pub use style::values::computed::OverflowWrap;
/// `text-align`, including the two values that depend on which value the parent had.
pub use style::values::computed::TextAlign;
/// `text-align-last`, how the final line of a block is aligned.
pub use style::values::computed::TextAlignLast;
/// `text-indent`, the inset of a paragraph's first line.
pub use style::values::computed::TextIndent;
/// `word-break`, which decides where a break is permitted at all.
pub use style::values::computed::WordBreak;
/// `word-spacing`, extra advance on space clusters.
pub use style::values::computed::WordSpacing;
/// The `text-align` values with no dependence on the parent's value.
pub use style::values::specified::text::TextAlignKeyword;

/// Builds a `letter-spacing` from a length.
///
/// The computed type is a generic one, so it cannot be constructed through its alias; this is the
/// constructor a caller wants.
pub fn letter_spacing(length: zgui_geom::CssPx) -> LetterSpacing {
    style::values::computed::text::GenericLetterSpacing(
        crate::values::length::LengthPercentage::new_length(crate::values::length::Length::new(
            length.0,
        )),
    )
}

/// Builds a `word-spacing` from a length.
pub fn word_spacing(length: zgui_geom::CssPx) -> WordSpacing {
    crate::values::length::LengthPercentage::new_length(crate::values::length::Length::new(
        length.0,
    ))
}

/// `text-justify`, which selects how a justified line distributes its extra space.
pub use style::values::computed::TextJustify;

/// `tab-size`, how far a preserved tab advances — either a count of space advances or a length.
pub use style::values::computed::NonNegativeLengthOrNumber as TabSize;

/// The three longhands `vertical-align` expands to, which is how this engine spells it.
///
/// `alignment-baseline` chooses which of the parent's baselines the box is aligned to,
/// `baseline-shift` moves it away from that baseline, and `baseline-source` selects which of a
/// multi-line box's own baselines is used. The legacy keywords land across all three: `middle`,
/// `text-top` and `text-bottom` are alignment baselines, `sub`, `super`, `top`, `bottom` and a
/// length are shifts, and `baseline` is every one of them at its initial value.
pub use style::values::computed::{AlignmentBaseline, BaselineShift, BaselineSource};

/// The keyword half of `baseline-shift`, for a shift that is not a length.
pub use style::values::generics::box_::BaselineShiftKeyword;

/// The computed value of `text-decoration-style`.
pub use style::computed_values::text_decoration_style::T as TextDecorationStyleValue;
/// The computed value of `text-decoration-line`, which is a set rather than one keyword: a box may
/// underline and strike through at once.
pub use style::values::computed::TextDecorationLine as TextDecorationLineValue;
