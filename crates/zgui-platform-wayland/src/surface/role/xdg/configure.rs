//! What the compositor asked a surface to become, and when it takes effect.

use std::num::NonZeroU32;

use wayland_protocols::xdg::shell::client::xdg_toplevel::State;

/// A configure that has arrived and not yet been applied.
///
/// The shell reports a change in two halves and they must not be acted on separately: the role
/// says what the surface should become, and the surface says the description is complete. Applying
/// the first half alone means resizing to an extent whose accompanying state has not arrived, and
/// acknowledging before the second half is a protocol error.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pending {
    /// The extent the compositor asked for, on each axis it named one.
    pub size: (Option<NonZeroU32>, Option<NonZeroU32>),
    /// Whether the window is maximised.
    pub maximized: bool,
    /// Whether the window fills the screen.
    pub fullscreen: bool,
    /// Whether the user is dragging an edge of it right now.
    pub resizing: bool,
    /// Whether the compositor has stopped repainting it.
    ///
    /// The state that this whole shell is bound at version six for. A compositor sends it only to
    /// a client that asked for a version new enough to receive it.
    pub suspended: bool,
    /// Whether the window has the keyboard.
    pub activated: bool,
}

impl Pending {
    /// The configure a set of states and an extent describe.
    ///
    /// The states arrive as an array of numbers rather than as a set, because the protocol has no
    /// set: unknown values are the compositor being newer than this client and are skipped, which
    /// is what lets a client bound at one version talk to a compositor at another.
    pub fn read(size: (i32, i32), states: &[u8]) -> Self {
        let mut pending = Self {
            size: (
                NonZeroU32::new(size.0.max(0) as u32),
                NonZeroU32::new(size.1.max(0) as u32),
            ),
            ..Self::default()
        };
        for state in named(states) {
            match state {
                State::Maximized => pending.maximized = true,
                State::Fullscreen => pending.fullscreen = true,
                State::Resizing => pending.resizing = true,
                State::Activated => pending.activated = true,
                State::Suspended => pending.suspended = true,
                _ => {}
            }
        }
        pending
    }
}

/// Every state in the array this client has a name for.
fn named(states: &[u8]) -> impl Iterator<Item = State> + '_ {
    states
        .chunks_exact(4)
        .filter_map(|chunk| <[u8; 4]>::try_from(chunk).ok())
        .map(u32::from_ne_bytes)
        .filter_map(|value| State::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use super::{Pending, named};
    use std::num::NonZeroU32;
    use wayland_protocols::xdg::shell::client::xdg_toplevel::State;

    fn states(values: &[u32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect()
    }

    #[test]
    fn a_configure_with_no_states_asks_for_nothing_in_particular() {
        let plain = Pending::read((800, 600), &[]);
        assert_eq!(plain.size.0, NonZeroU32::new(800));
        assert!(!plain.maximized && !plain.suspended && !plain.activated);
    }

    #[test]
    fn an_extent_of_zero_is_the_compositor_leaving_the_choice_to_the_window() {
        // Not a window of no size: zero on an axis means "whatever you like".
        let free = Pending::read((0, 0), &[]);
        assert_eq!(free.size, (None, None));
    }

    #[test]
    fn every_state_this_client_acts_on_is_read_out_of_the_array() {
        let all = states(&[
            State::Maximized as u32,
            State::Fullscreen as u32,
            State::Resizing as u32,
            State::Activated as u32,
            State::Suspended as u32,
        ]);
        let read = Pending::read((0, 0), &all);
        assert!(read.maximized && read.fullscreen && read.resizing);
        assert!(read.activated && read.suspended);
    }

    #[test]
    fn suspension_is_read_and_is_not_confused_with_anything_else() {
        // The state the whole shell is bound at version six for.
        let only = Pending::read((0, 0), &states(&[State::Suspended as u32]));
        assert!(only.suspended);
        assert!(!only.maximized && !only.activated && !only.fullscreen);
    }

    #[test]
    fn a_state_this_client_has_no_name_for_is_skipped_rather_than_refused() {
        // A compositor newer than this client sends states it has never heard of, and a client
        // that stopped reading at the first would lose every state after it.
        let future = states(&[9_999, State::Suspended as u32]);
        assert_eq!(named(&future).count(), 1);
        assert!(Pending::read((0, 0), &future).suspended);
    }

    #[test]
    fn a_truncated_array_is_read_as_far_as_it_goes() {
        let mut broken = states(&[State::Maximized as u32]);
        broken.push(0);
        assert!(Pending::read((0, 0), &broken).maximized);
    }
}
