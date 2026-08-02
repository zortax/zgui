//! Boxes for generated content, and the mark a list item carries.
//!
//! This is the only place a box acquires a pseudo-element identity, and the only place a mark box
//! is made. Both are boxes with no element of their own: they name the element they were generated
//! *from*, so a click on generated content resolves to the originating element, its accessible
//! geometry joins that element's, and nothing downstream needs a case for them.

use zgui_css::ComputedStyle;
use zgui_css::values::content::{ContentItemValue, ContentValue};
use zgui_css::values::list::ListStyleTypeValue;
use zgui_dom::NodeKey;

use crate::boxtree::anonymous::synthesised_style;
use crate::node::box_node::BoxNode;
use crate::node::kind::{BoxKind, FormattingContext, PseudoKind};
use crate::style::convert::display;

/// The text a `content` value lowers to, or nothing if it places no text.
///
/// Only the literal strings are lowered. A counter needs a resolution pass over the whole document
/// and a quote needs the nesting depth, neither of which is a property of one element.
pub fn content_text(style: &ComputedStyle) -> Option<String> {
    let ContentValue::Items(items) = &style.get_counters().content else {
        return None;
    };
    let mut text = String::new();
    for item in items.items.iter().take(items.alt_start) {
        if let ContentItemValue::String(literal) = item {
            text.push_str(literal);
        }
    }
    Some(text)
}

/// The box one generated-content pseudo-element produces, if it produces one.
///
/// The caller has already established that the pseudo-element exists — a style is stored for it and
/// it would generate something — so there is no second existence test here.
pub fn generated_box(source: NodeKey, kind: PseudoKind, style: &ComputedStyle) -> BoxNode {
    let fc = display::formatting_context(style.get_box().display);
    let mut node = BoxNode::new(style.clone(), BoxKind::Element, fc)
        .from_element(source)
        .as_pseudo(kind);
    node.block_level =
        display::participation(style.get_box().display) == display::Participation::Block;
    node
}

/// The box a list item marks itself with.
///
/// The mark is styled by the list item's own inherited `list-style-*` properties over a synthesised
/// style, not by a pseudo-element: the pseudo-element that would carry author styling for it is
/// resolved lazily and no traversal computes one, while `list-style-type`, `list-style-image` and
/// `list-style-position` are inherited properties of the item itself and are computed for every
/// element already.
pub fn marker_box(source: NodeKey, item_style: &ComputedStyle) -> BoxNode {
    let mut node = BoxNode::new(
        synthesised_style(item_style),
        BoxKind::Marker,
        FormattingContext::Inline,
    )
    .from_element(source);
    node.text = marker_text(item_style).map(String::into_boxed_str);
    node
}

/// The text a mark shows, or nothing for a mark that shows an image or nothing at all.
fn marker_text(style: &ComputedStyle) -> Option<String> {
    match &style.get_list().list_style_type {
        value if *value == ListStyleTypeValue::none() => None,
        // Every counter style beyond the bullet needs the item's index within its list, which is a
        // property of the list rather than of the item.
        _ => Some("\u{2022}".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::StyleDraft;
    use zgui_css::values::list::ListStyleTypeValue;

    use super::{content_text, marker_text};

    #[test]
    fn a_style_with_no_content_lowers_to_no_text_at_all() {
        // `normal` and `none` are different values and neither places anything, so the answer is
        // the absence of a box rather than an empty one.
        assert_eq!(content_text(&StyleDraft::initial().build()), None);
    }

    #[test]
    fn a_mark_with_no_type_shows_nothing() {
        let mut draft = StyleDraft::initial();
        draft.list().list_style_type = ListStyleTypeValue::none();
        assert_eq!(marker_text(&draft.build()), None);
    }

    #[test]
    fn the_default_mark_is_a_bullet() {
        assert_eq!(
            marker_text(&StyleDraft::initial().build()),
            Some("\u{2022}".to_owned())
        );
    }
}
