//! What a key press means to a control whose options are somewhere else.

use zgui::vocab::{Key, NamedKey};

/// What one key press asks a listbox to do.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ListboxAction {
    /// Move the highlight this many places, opening the list first if it is closed.
    Step(isize),
    /// Move it to the last option, or the first.
    End(bool),
    /// Choose whatever is highlighted.
    Choose,
    /// Close the list without choosing.
    Close,
}

/// What `key` means to a listbox that is open, or closed.
///
/// The two differ in one place and it matters: the down arrow on a **closed** select opens it
/// rather than moving anything, and Enter on a closed one does nothing at all — a form whose
/// select swallowed Enter would be a form that cannot be submitted from the keyboard.
///
/// <kbd>Tab</kbd> is deliberately absent. A control that claimed it would be one nobody can leave,
/// and the list closing on the way past is the dismissal's business rather than a key's.
///
/// ```
/// use zgui::vocab::{Key, NamedKey};
/// use zgui_ui::listbox::{ListboxAction, action_for};
///
/// let down = Key::Named(NamedKey::ArrowDown);
/// assert_eq!(action_for(&down, false), Some(ListboxAction::Step(1)));
/// assert_eq!(action_for(&down, true), Some(ListboxAction::Step(1)));
///
/// let enter = Key::Named(NamedKey::Enter);
/// assert_eq!(action_for(&enter, true), Some(ListboxAction::Choose));
/// assert_eq!(action_for(&enter, false), None, "a closed list has nothing to choose");
/// ```
#[must_use]
pub fn action_for(key: &Key, open: bool) -> Option<ListboxAction> {
    let Key::Named(named) = key else {
        return None;
    };
    match named {
        NamedKey::ArrowDown => Some(ListboxAction::Step(1)),
        NamedKey::ArrowUp => Some(ListboxAction::Step(-1)),
        NamedKey::Home if open => Some(ListboxAction::End(false)),
        NamedKey::End if open => Some(ListboxAction::End(true)),
        NamedKey::PageDown if open => Some(ListboxAction::Step(10)),
        NamedKey::PageUp if open => Some(ListboxAction::Step(-10)),
        NamedKey::Enter if open => Some(ListboxAction::Choose),
        NamedKey::Escape if open => Some(ListboxAction::Close),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use zgui::vocab::{Key, NamedKey};

    use super::{ListboxAction, action_for};

    fn named(key: NamedKey) -> Key {
        Key::Named(key)
    }

    #[test]
    fn a_closed_list_answers_the_arrows_and_nothing_else() {
        // The down arrow opens it, which is why it answers; every other key belongs to whatever is
        // around the control until there is a list to operate.
        assert_eq!(
            action_for(&named(NamedKey::ArrowDown), false),
            Some(ListboxAction::Step(1))
        );
        for key in [
            NamedKey::Home,
            NamedKey::End,
            NamedKey::Enter,
            NamedKey::Escape,
            NamedKey::PageDown,
        ] {
            assert_eq!(
                action_for(&named(key), false),
                None,
                "{key:?} on a closed list"
            );
        }
    }

    #[test]
    fn nothing_here_ever_claims_tab() {
        // A control that swallowed Tab is a control nobody can leave, and a select in a form is
        // exactly where that is fatal.
        assert_eq!(action_for(&named(NamedKey::Tab), true), None);
        assert_eq!(action_for(&named(NamedKey::Tab), false), None);
    }

    #[test]
    fn typing_a_letter_is_not_a_listbox_action() {
        // It is a typeahead or a filter, and which of the two depends on the control rather than
        // on the key — so this answers neither.
        assert_eq!(action_for(&Key::Character("a".into()), true), None);
    }

    #[test]
    fn the_page_keys_move_by_ten_and_the_ends_go_to_the_ends() {
        assert_eq!(
            action_for(&named(NamedKey::PageDown), true),
            Some(ListboxAction::Step(10))
        );
        assert_eq!(
            action_for(&named(NamedKey::PageUp), true),
            Some(ListboxAction::Step(-10))
        );
        assert_eq!(
            action_for(&named(NamedKey::Home), true),
            Some(ListboxAction::End(false))
        );
        assert_eq!(
            action_for(&named(NamedKey::End), true),
            Some(ListboxAction::End(true))
        );
    }
}
