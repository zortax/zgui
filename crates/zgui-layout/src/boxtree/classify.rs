//! Deciding what kind of box an element generates, before any box is made.

use zgui_css::ComputedStyle;
use zgui_css::values::size::PositionValue;

use crate::node::kind::FormattingContext;
use crate::style::convert::display::{self, Participation};

/// What one element's style says about the box it generates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Classification {
    /// How the box takes part in the context around it.
    pub participation: Participation,
    /// The rules the box lays its own children out by.
    pub fc: FormattingContext,
    /// Whether the box is taken out of normal flow and positioned against a containing block.
    pub out_of_flow: bool,
    /// Whether the box establishes a containing block for absolutely positioned descendants.
    pub positioned: bool,
    /// Whether the box generates a mark of its own.
    pub list_item: bool,
}

/// Classifies one style.
pub fn classify(style: &ComputedStyle) -> Classification {
    let display = style.get_box().display;
    let position = style.get_box().position;
    Classification {
        participation: display::participation(display),
        fc: display::formatting_context(display),
        out_of_flow: matches!(position, PositionValue::Absolute | PositionValue::Fixed),
        positioned: !matches!(position, PositionValue::Static),
        list_item: display::is_list_item(display),
    }
}

/// The classification an inline-level box takes on when it becomes a flex or grid item.
///
/// Flex and grid items are blockified: there is no line for an inline-level box to sit in, so
/// `inline` becomes `block`, `inline-flex` becomes `flex` and `inline-block` becomes `block`.
/// Doing it here as well as wherever the cascade does it is not redundant — an anonymous box never
/// went through a cascade, and one that stayed inline-level would be wrapped in a line that its
/// container has no way to lay out.
#[must_use]
pub fn blockify(mut classification: Classification) -> Classification {
    if classification.participation != Participation::Inline {
        return classification;
    }
    classification.participation = Participation::Block;
    classification.fc = match classification.fc {
        FormattingContext::Inline => FormattingContext::Block,
        FormattingContext::Atomic => FormattingContext::Block,
        other => other,
    };
    classification
}

/// The same, for an atomic inline whose inner display decides what runs inside it.
#[must_use]
pub fn blockify_with(classification: Classification, style: &ComputedStyle) -> Classification {
    if classification.participation != Participation::Inline
        || classification.fc != FormattingContext::Atomic
    {
        return blockify(classification);
    }
    let mut blockified = classification;
    blockified.participation = Participation::Block;
    blockified.fc = display::atomic_inner(style.get_box().display);
    blockified
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;

    use crate::node::kind::FormattingContext;
    use crate::style::convert::display::Participation;

    use super::{blockify, blockify_with, classify};

    #[test]
    fn the_initial_style_is_an_inline_box_in_normal_flow() {
        let classification = classify(&StyleDraft::initial().build());
        assert_eq!(classification.participation, Participation::Inline);
        assert_eq!(classification.fc, FormattingContext::Inline);
        assert!(!classification.out_of_flow);
        assert!(!classification.positioned);
        assert!(!classification.list_item);
    }

    #[test]
    fn blockifying_an_inline_box_makes_it_a_block_container() {
        let classification = classify(&StyleDraft::initial().build());
        let blockified = blockify(classification);
        assert_eq!(blockified.participation, Participation::Block);
        assert_eq!(blockified.fc, FormattingContext::Block);
    }

    #[test]
    fn blockifying_a_block_level_box_changes_nothing() {
        let mut classification = classify(&StyleDraft::initial().build());
        classification.participation = Participation::Block;
        classification.fc = FormattingContext::Flex;
        assert_eq!(blockify(classification), classification);
    }

    #[test]
    fn an_atomic_inline_keeps_what_ran_inside_it() {
        let style = StyleDraft::initial().build();
        let mut classification = classify(&style);
        classification.fc = FormattingContext::Atomic;
        // The initial `display` has a flow inner value, so the box that survives is a block one.
        let blockified = blockify_with(classification, &style);
        assert_eq!(blockified.participation, Participation::Block);
        assert_eq!(blockified.fc, FormattingContext::Block);
    }
}
