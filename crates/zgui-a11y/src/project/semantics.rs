//! What an element declares about itself, written onto an accessibility node.
//!
//! Every value here is already one of accesskit's own, so this is a transcription and not a
//! translation. That is deliberate: a parallel enumeration would have to be kept in step by hand
//! and would convert on every property of every node.

use accesskit::Node;
use zgui_vocab::{SemanticFlags, Semantics};

/// Writes everything `semantics` declares onto `into`.
///
/// The role is the caller's, because a text node's role comes from what it is rather than from
/// what it declared.
pub fn apply(semantics: &Semantics, into: &mut Node) {
    strings(semantics, into);
    flags(semantics.flags, into);
    tri_state(semantics, into);
    enums(semantics, into);
    numbers(semantics, into);
}

/// The properties that are text.
fn strings(semantics: &Semantics, into: &mut Node) {
    if let Some(label) = &semantics.label {
        into.set_label(label.as_str());
    }
    if let Some(description) = &semantics.description {
        into.set_description(description.as_str());
    }
    if let Some(value) = &semantics.value {
        into.set_value(value.as_str());
    }
    if let Some(placeholder) = &semantics.placeholder {
        into.set_placeholder(placeholder.as_str());
    }
    if let Some(role_description) = &semantics.role_description {
        into.set_role_description(role_description.as_str());
    }
    if let Some(state_description) = &semantics.state_description {
        into.set_state_description(state_description.as_str());
    }
    if let Some(shortcut) = &semantics.keyboard_shortcut {
        into.set_keyboard_shortcut(shortcut.as_str());
    }
    if let Some(access_key) = &semantics.access_key {
        into.set_access_key(access_key.as_str());
    }
    if let Some(tooltip) = &semantics.tooltip {
        into.set_tooltip(tooltip.as_str());
    }
}

/// The properties that are simply on or off.
///
/// `CLIPS_CHILDREN` is absent on purpose: whether a box clips is a fact about its overflow, which
/// the fragment tree already knows, and taking it from a declaration as well would let the two
/// disagree.
fn flags(set: SemanticFlags, into: &mut Node) {
    let table = [
        (SemanticFlags::HIDDEN, Node::set_hidden as fn(&mut Node)),
        (SemanticFlags::DISABLED, Node::set_disabled),
        (SemanticFlags::READ_ONLY, Node::set_read_only),
        (SemanticFlags::REQUIRED, Node::set_required),
        (SemanticFlags::MULTISELECTABLE, Node::set_multiselectable),
        (SemanticFlags::MODAL, Node::set_modal),
        (SemanticFlags::BUSY, Node::set_busy),
        (SemanticFlags::LIVE_ATOMIC, Node::set_live_atomic),
        (
            SemanticFlags::TOUCH_TRANSPARENT,
            Node::set_touch_transparent,
        ),
        (SemanticFlags::VISITED, Node::set_visited),
    ];
    for (flag, set_it) in table {
        if set.contains(flag) {
            set_it(into);
        }
    }
}

/// The properties where "has not said" and "said no" are different statements.
fn tri_state(semantics: &Semantics, into: &mut Node) {
    if let Some(expanded) = semantics.expanded {
        into.set_expanded(expanded);
    }
    if let Some(selected) = semantics.selected {
        into.set_selected(selected);
    }
    if let Some(toggled) = semantics.toggled {
        into.set_toggled(toggled);
    }
}

/// The properties that are a small closed set of alternatives.
fn enums(semantics: &Semantics, into: &mut Node) {
    if let Some(invalid) = semantics.invalid {
        into.set_invalid(invalid);
    }
    if let Some(live) = semantics.live {
        into.set_live(live);
    }
    if let Some(orientation) = semantics.orientation {
        into.set_orientation(orientation);
    }
    if let Some(has_popup) = semantics.has_popup {
        into.set_has_popup(has_popup);
    }
    if let Some(auto_complete) = semantics.auto_complete {
        into.set_auto_complete(auto_complete);
    }
    if let Some(sort) = semantics.sort_direction {
        into.set_sort_direction(sort);
    }
    if let Some(current) = semantics.current {
        into.set_aria_current(current);
    }
}

/// The properties that are numbers: a measured value, a position in a set, a place in a table.
fn numbers(semantics: &Semantics, into: &mut Node) {
    let numeric = semantics.numeric;
    if let Some(value) = numeric.value {
        into.set_numeric_value(value);
    }
    if let Some(min) = numeric.min {
        into.set_min_numeric_value(min);
    }
    if let Some(max) = numeric.max {
        into.set_max_numeric_value(max);
    }
    if let Some(step) = numeric.step {
        into.set_numeric_value_step(step);
    }
    if let Some(jump) = numeric.jump {
        into.set_numeric_value_jump(jump);
    }

    let position = semantics.position;
    if let Some(index) = position.position_in_set {
        into.set_position_in_set(index);
    }
    if let Some(size) = position.size_of_set {
        into.set_size_of_set(size);
    }
    if let Some(level) = position.level {
        into.set_level(level);
    }

    let table = semantics.table;
    if let Some(rows) = table.row_count {
        into.set_row_count(rows);
    }
    if let Some(columns) = table.column_count {
        into.set_column_count(columns);
    }
    if let Some(row) = table.row_index {
        into.set_row_index(row);
    }
    if let Some(column) = table.column_index {
        into.set_column_index(column);
    }
    if let Some(span) = table.row_span {
        into.set_row_span(span);
    }
    if let Some(span) = table.column_span {
        into.set_column_span(span);
    }
}

#[cfg(test)]
mod tests {
    use accesskit::{Node, Role, Toggled};
    use zgui_vocab::{A11y, SemanticFlags, Semantics};

    use super::apply;

    #[test]
    fn a_declaration_reaches_every_property_group() {
        let semantics: Semantics = A11y::new(Role::CheckBox)
            .label("Ship it")
            .description("Publishes the build")
            .toggled(Toggled::Mixed)
            .disabled(true)
            .level(3usize)
            .into();
        let mut node = Node::new(Role::CheckBox);
        apply(&semantics, &mut node);

        assert_eq!(node.label(), Some("Ship it"));
        assert_eq!(node.description(), Some("Publishes the build"));
        assert_eq!(node.toggled(), Some(Toggled::Mixed));
        assert!(node.is_disabled());
        assert_eq!(node.level(), Some(3));
    }

    #[test]
    fn saying_nothing_writes_nothing() {
        let mut node = Node::new(Role::GenericContainer);
        apply(&Semantics::default(), &mut node);
        assert_eq!(node, Node::new(Role::GenericContainer));
    }

    #[test]
    fn a_collapsed_control_is_distinguishable_from_one_that_does_not_expand() {
        let mut collapsed = Node::new(Role::Button);
        apply(
            &Semantics {
                expanded: Some(false),
                ..Semantics::new(Role::Button)
            },
            &mut collapsed,
        );
        let mut silent = Node::new(Role::Button);
        apply(&Semantics::new(Role::Button), &mut silent);

        assert_eq!(collapsed.is_expanded(), Some(false));
        assert_eq!(silent.is_expanded(), None);
    }

    #[test]
    fn clipping_is_never_taken_from_a_declaration() {
        let mut node = Node::new(Role::GenericContainer);
        apply(
            &Semantics {
                flags: SemanticFlags::CLIPS_CHILDREN,
                ..Semantics::default()
            },
            &mut node,
        );
        assert!(
            !node.clips_children(),
            "whether a box clips is the fragment tree's answer; a second source would disagree \
             with it"
        );
    }
}
