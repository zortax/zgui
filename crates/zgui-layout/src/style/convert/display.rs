//! `display`, split into the two questions a box tree actually asks.
//!
//! The property packs two independent things: how a box participates in its parent's formatting
//! context, and what formatting context it establishes for its own children. The box tree needs
//! both separately — the first decides whether a run of siblings needs an anonymous wrapper, the
//! second decides which algorithm lays the box out — so nothing here returns a single verdict.

use zgui_css::values::size::{DisplayInside, DisplayOutside, DisplayValue};

use crate::node::kind::FormattingContext;

/// How a box takes part in the context around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Participation {
    /// No box at all.
    None,
    /// No box, but the children take the parent's place in the tree.
    Contents,
    /// A block-level box: it takes a line of its own.
    Block,
    /// An inline-level box: it sits in a line with its siblings.
    Inline,
}

/// How a box takes part in the context around it.
pub fn participation(display: DisplayValue) -> Participation {
    match display.inside() {
        DisplayInside::None => Participation::None,
        DisplayInside::Contents => Participation::Contents,
        _ => match display.outside() {
            DisplayOutside::None => Participation::None,
            DisplayOutside::Inline => Participation::Inline,
            DisplayOutside::Block
            | DisplayOutside::TableCaption
            | DisplayOutside::InternalTable => Participation::Block,
        },
    }
}

/// The rules a box lays its own children out by.
///
/// An inline-level box whose inner display is not `flow` is *atomic*: a leaf to the line it sits
/// in and a container to everything inside it. That distinction is the whole reason this returns a
/// formatting context rather than a copy of the inner display value.
pub fn formatting_context(display: DisplayValue) -> FormattingContext {
    let inline_level = display.outside() == DisplayOutside::Inline;
    match display.inside() {
        DisplayInside::None => FormattingContext::None,
        // A box whose children reparent onto its own parent generates no box, so it never reaches
        // a formatting context; the box tree drops it before this is asked.
        DisplayInside::Contents => FormattingContext::None,
        DisplayInside::Flow | DisplayInside::FlowRoot => {
            if inline_level && display.inside() == DisplayInside::Flow {
                FormattingContext::Inline
            } else if inline_level {
                FormattingContext::Atomic
            } else {
                FormattingContext::Block
            }
        }
        DisplayInside::Flex => {
            if inline_level {
                FormattingContext::Atomic
            } else {
                FormattingContext::Flex
            }
        }
        DisplayInside::Grid => {
            if inline_level {
                FormattingContext::Atomic
            } else {
                FormattingContext::Grid
            }
        }
        DisplayInside::Table
        | DisplayInside::TableRowGroup
        | DisplayInside::TableColumn
        | DisplayInside::TableColumnGroup
        | DisplayInside::TableHeaderGroup
        | DisplayInside::TableFooterGroup
        | DisplayInside::TableRow
        | DisplayInside::TableCell => FormattingContext::Table,
    }
}

/// The formatting context an atomic inline runs *inside itself*.
///
/// An atomic inline is a leaf to the line around it, but its own children are laid out by whatever
/// its inner display says — so the atomic case needs this second answer as well as the first.
pub fn atomic_inner(display: DisplayValue) -> FormattingContext {
    match display.inside() {
        DisplayInside::Flex => FormattingContext::Flex,
        DisplayInside::Grid => FormattingContext::Grid,
        DisplayInside::Table => FormattingContext::Table,
        _ => FormattingContext::Block,
    }
}

/// Whether a box generates a mark of its own.
pub fn is_list_item(display: DisplayValue) -> bool {
    display.is_list_item()
}

#[cfg(test)]
mod tests {
    use zgui_css::values::size::DisplayValue;

    use crate::node::kind::FormattingContext;

    use super::{Participation, atomic_inner, formatting_context, participation};

    #[test]
    fn the_two_questions_are_answered_independently() {
        assert_eq!(
            participation(DisplayValue::InlineBlock),
            Participation::Inline
        );
        assert_eq!(
            formatting_context(DisplayValue::InlineBlock),
            FormattingContext::Atomic
        );
        assert_eq!(participation(DisplayValue::Block), Participation::Block);
        assert_eq!(
            formatting_context(DisplayValue::Block),
            FormattingContext::Block
        );
    }

    #[test]
    fn plain_inline_is_the_only_inline_level_box_that_is_not_atomic() {
        assert_eq!(
            formatting_context(DisplayValue::Inline),
            FormattingContext::Inline
        );
        for display in [
            DisplayValue::InlineBlock,
            DisplayValue::InlineFlex,
            DisplayValue::InlineGrid,
        ] {
            assert_eq!(
                formatting_context(display),
                FormattingContext::Atomic,
                "{display:?}"
            );
        }
    }

    #[test]
    fn an_atomic_inline_knows_which_algorithm_runs_inside_it() {
        assert_eq!(
            atomic_inner(DisplayValue::InlineFlex),
            FormattingContext::Flex
        );
        assert_eq!(
            atomic_inner(DisplayValue::InlineGrid),
            FormattingContext::Grid
        );
        assert_eq!(
            atomic_inner(DisplayValue::InlineBlock),
            FormattingContext::Block
        );
    }

    #[test]
    fn none_and_contents_are_different_answers() {
        assert_eq!(participation(DisplayValue::None), Participation::None);
        assert_eq!(
            participation(DisplayValue::Contents),
            Participation::Contents
        );
    }

    #[test]
    fn a_flex_container_is_block_level_and_lays_its_children_out_by_flex() {
        assert_eq!(participation(DisplayValue::Flex), Participation::Block);
        assert_eq!(
            formatting_context(DisplayValue::Flex),
            FormattingContext::Flex
        );
        assert_eq!(
            formatting_context(DisplayValue::Grid),
            FormattingContext::Grid
        );
    }
}
