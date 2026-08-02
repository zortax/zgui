//! Classifying a style change as a re-shape, a re-break, or neither.

use zgui_css::ComputedStyle;

use crate::key::{BreakingKey, ShapingKey};
use crate::lower::set::{paragraph_style, text_style};

/// The text work a change from one style to another costs.
///
/// The two levels are two orders of magnitude apart in cost, which is why they are distinguished at
/// all rather than reported as one "text changed" bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextDamage {
    /// Nothing about the text changed. Any difference between the two styles is a difference the
    /// text pipeline does not read — a colour, a background, a border.
    None,
    /// The lines have to be laid out again, reusing the glyphs already produced.
    Rebreak,
    /// The text has to be shaped again, and broken again after that.
    Reshape,
}

impl TextDamage {
    /// A change that costs a fresh shape, which implies a fresh break.
    pub const RESHAPE: Self = Self::Reshape;
    /// A change that costs only a fresh break.
    pub const REBREAK: Self = Self::Rebreak;

    /// The work a change from `old` to `new` costs.
    ///
    /// This is the shaping-key / breaking-key split applied to two whole styles, and it is derived
    /// from those keys rather than from a table of properties — so a property cannot be classified
    /// one way here and hashed the other way there.
    pub fn between(old: &ComputedStyle, new: &ComputedStyle) -> Self {
        let shaped_the_same = ShapingKey::of(&text_style(old)) == ShapingKey::of(&text_style(new))
            && ShapingKey::of_paragraph(&paragraph_style(old))
                == ShapingKey::of_paragraph(&paragraph_style(new));
        if !shaped_the_same {
            return Self::Reshape;
        }
        let broke_the_same = BreakingKey::of(&text_style(old)) == BreakingKey::of(&text_style(new))
            && BreakingKey::of_paragraph(&paragraph_style(old))
                == BreakingKey::of_paragraph(&paragraph_style(new));
        if broke_the_same {
            Self::None
        } else {
            Self::Rebreak
        }
    }

    /// Whether the change costs a shaping pass.
    pub fn reshapes(self) -> bool {
        self == Self::Reshape
    }

    /// Whether the change costs at least a breaking pass.
    pub fn rebreaks(self) -> bool {
        self != Self::None
    }
}
