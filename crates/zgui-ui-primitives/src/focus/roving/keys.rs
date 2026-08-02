//! Which key means which move, and along which axis.

use zgui::vocab::{Key, NamedKey};

/// Which arrow keys move within a group.
///
/// The keys for the *other* axis are deliberately left alone rather than treated as the same move.
/// A vertical menu that also answered the left and right arrows would swallow the keys a
/// horizontal menubar above it needs, and the submenu would become impossible to leave.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum Orientation {
    /// Left and right.
    #[default]
    Horizontal,
    /// Up and down.
    Vertical,
    /// Both, for a grid or a group whose direction is not fixed.
    Both,
}

impl Orientation {
    /// How this is written as an attribute value, which is what a style sheet selects on.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::Vertical => "vertical",
            Self::Both => "both",
        }
    }

    /// Whether the left and right arrows move within a group of this orientation.
    const fn takes_horizontal(self) -> bool {
        matches!(self, Self::Horizontal | Self::Both)
    }

    /// Whether the up and down arrows do.
    const fn takes_vertical(self) -> bool {
        matches!(self, Self::Vertical | Self::Both)
    }
}

/// What a key press means inside a group.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Action {
    /// Move this many places.
    Step(isize),
    /// Go to the last item, or the first.
    End(bool),
}

/// What `key` means in a group of this orientation, when it means anything.
pub(crate) fn action(key: &Key, orientation: Orientation) -> Option<Action> {
    let named = match key {
        Key::Named(named) => *named,
        Key::Character(_) => return None,
        _ => return None,
    };
    match named {
        NamedKey::ArrowRight if orientation.takes_horizontal() => Some(Action::Step(1)),
        NamedKey::ArrowLeft if orientation.takes_horizontal() => Some(Action::Step(-1)),
        NamedKey::ArrowDown if orientation.takes_vertical() => Some(Action::Step(1)),
        NamedKey::ArrowUp if orientation.takes_vertical() => Some(Action::Step(-1)),
        NamedKey::Home => Some(Action::End(false)),
        NamedKey::End => Some(Action::End(true)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use zgui::vocab::{Key, NamedKey};

    use super::{Action, Orientation, action};

    fn named(key: NamedKey) -> Key {
        Key::Named(key)
    }

    #[test]
    fn a_horizontal_group_leaves_the_vertical_arrows_to_whatever_is_outside_it() {
        // The case this protects: a horizontal menubar with vertical submenus. If the bar
        // answered the down arrow, the submenu could never be entered.
        assert_eq!(
            action(&named(NamedKey::ArrowRight), Orientation::Horizontal),
            Some(Action::Step(1))
        );
        assert_eq!(
            action(&named(NamedKey::ArrowDown), Orientation::Horizontal),
            None
        );
        assert_eq!(
            action(&named(NamedKey::ArrowUp), Orientation::Horizontal),
            None
        );
    }

    #[test]
    fn a_vertical_group_leaves_the_horizontal_arrows_alone() {
        assert_eq!(
            action(&named(NamedKey::ArrowDown), Orientation::Vertical),
            Some(Action::Step(1))
        );
        assert_eq!(
            action(&named(NamedKey::ArrowUp), Orientation::Vertical),
            Some(Action::Step(-1))
        );
        assert_eq!(
            action(&named(NamedKey::ArrowRight), Orientation::Vertical),
            None
        );
    }

    #[test]
    fn a_group_in_both_directions_answers_all_four() {
        for (key, expected) in [
            (NamedKey::ArrowRight, Action::Step(1)),
            (NamedKey::ArrowLeft, Action::Step(-1)),
            (NamedKey::ArrowDown, Action::Step(1)),
            (NamedKey::ArrowUp, Action::Step(-1)),
        ] {
            assert_eq!(action(&named(key), Orientation::Both), Some(expected));
        }
    }

    #[test]
    fn the_ends_are_the_ends_whichever_way_the_group_runs() {
        for orientation in [
            Orientation::Horizontal,
            Orientation::Vertical,
            Orientation::Both,
        ] {
            assert_eq!(
                action(&named(NamedKey::Home), orientation),
                Some(Action::End(false))
            );
            assert_eq!(
                action(&named(NamedKey::End), orientation),
                Some(Action::End(true))
            );
        }
    }

    #[test]
    fn a_key_that_means_nothing_here_means_nothing() {
        assert_eq!(action(&named(NamedKey::Tab), Orientation::Both), None);
        assert_eq!(action(&named(NamedKey::Escape), Orientation::Both), None);
        assert_eq!(action(&Key::Character("a".into()), Orientation::Both), None);
    }

    #[test]
    fn every_orientation_has_a_distinct_attribute_value() {
        let mut names = [
            Orientation::Horizontal.name(),
            Orientation::Vertical.name(),
            Orientation::Both.name(),
        ];
        names.sort_unstable();
        let distinct = {
            let mut seen = names;
            seen.sort_unstable();
            seen.iter().collect::<std::collections::BTreeSet<_>>().len()
        };
        assert_eq!(distinct, 3);
    }
}
