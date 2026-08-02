//! Pushing a toast aside with the pointer.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};

/// How far the pointer has to travel before this is a swipe and not a press, in CSS pixels.
///
/// A gesture that took the pointer the moment a button went down would take it from the close button
/// as well: the press would be captured by the toast, the release would be delivered to the toast
/// rather than to the control under the pointer, and the click would never happen. So the toast waits
/// to be sure, and a press that never travels is left alone to become somebody else's click.
const SLOP: f32 = 6.0;

/// How far a toast has to be pushed before letting go dismisses it, in CSS pixels.
const THRESHOLD: f32 = 45.0;

/// What letting go of a toast asks for.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum LetGo {
    /// It was pushed far enough: take it away.
    Dismiss,
    /// It was not: put it back.
    Restore,
    /// Nothing was being pushed, so the press belongs to whatever is under the pointer.
    Nothing,
}

/// How far the pointer has pushed one toast, and whether it has taken it over.
///
/// Both distances are in CSS pixels, which is the unit the pointer reports and the unit a style sheet
/// is written in.
#[derive(Copy, Clone)]
pub(crate) struct Swipe {
    /// Where the press started, while a button is down.
    from: RwSignal<Option<f32>, LocalStorage>,
    /// How far it has gone since.
    pushed: RwSignal<f32, LocalStorage>,
    /// Whether the travel has passed [`SLOP`] and this is now a swipe.
    swiping: RwSignal<bool, LocalStorage>,
}

impl Swipe {
    /// A gesture that has not started.
    pub(crate) fn new() -> Self {
        Self {
            from: RwSignal::new_local(None),
            pushed: RwSignal::new_local(0.0),
            swiping: RwSignal::new_local(false),
        }
    }

    /// Notes a press at `x`.
    pub(crate) fn press(self, x: f32) {
        self.from.set(Some(x));
        self.swiping.set(false);
    }

    /// Notes the pointer reaching `x`, and says whether the toast has just taken it over.
    ///
    /// `true` exactly once per gesture: on the move that passes [`SLOP`]. That is the moment the
    /// caller captures the pointer, so the rest of the swipe survives leaving the toast's own box.
    pub(crate) fn moved(self, x: f32) -> bool {
        let Some(from) = self.from.get_untracked() else {
            return false;
        };
        let travelled = x - from;
        if !self.swiping.get_untracked() {
            if travelled.abs() < SLOP {
                return false;
            }
            self.swiping.set(true);
            self.pushed.set(travelled);
            return true;
        }
        self.pushed.set(travelled);
        false
    }

    /// Notes the button coming up, and says what it asks for.
    pub(crate) fn let_go(self) -> LetGo {
        let swiping = self.swiping.get_untracked();
        let pushed = self.pushed.get_untracked();
        self.from.set(None);
        self.swiping.set(false);
        if !swiping {
            return LetGo::Nothing;
        }
        if pushed.abs() >= THRESHOLD {
            LetGo::Dismiss
        } else {
            self.pushed.set(0.0);
            LetGo::Restore
        }
    }

    /// Notes the gesture being taken over by something else, which produces no release.
    pub(crate) fn cancel(self) {
        self.from.set(None);
        self.swiping.set(false);
        self.pushed.set(0.0);
    }

    /// How far the toast is pushed, in CSS pixels.
    pub(crate) fn distance(self) -> f32 {
        self.pushed.get()
    }

    /// Whether the toast is following the pointer, which is what turns its transition off.
    pub(crate) fn is_swiping(self) -> bool {
        self.swiping.get()
    }
}

#[cfg(test)]
mod tests {
    use zgui::reactive::{Mounted, install};

    use super::{LetGo, Swipe};

    /// Runs `body` inside a mounted reactive scope, which is what signals need.
    fn mounted(body: impl FnOnce()) {
        install().ok();
        let scope = Mounted::new();
        scope.with(body);
        scope.unmount();
    }

    #[test]
    fn a_press_that_never_travels_is_not_a_swipe() {
        // The defect this prevents is the close button never receiving a click: a gesture that took
        // the pointer on the press would be handed the release that the button was waiting for.
        mounted(|| {
            let swipe = Swipe::new();
            swipe.press(100.0);
            assert!(!swipe.moved(101.0), "one pixel is not a gesture");
            assert!(!swipe.is_swiping());
            assert_eq!(swipe.distance(), 0.0);
            assert_eq!(swipe.let_go(), LetGo::Nothing);
        });
    }

    #[test]
    fn the_pointer_is_taken_over_once_and_on_the_move_that_passes_the_slop() {
        mounted(|| {
            let swipe = Swipe::new();
            swipe.press(100.0);
            assert!(!swipe.moved(104.0));
            assert!(swipe.moved(110.0), "this is where the capture happens");
            assert!(!swipe.moved(140.0), "and it is not asked for twice");
            assert_eq!(swipe.distance(), 40.0);
        });
    }

    #[test]
    fn a_short_push_puts_the_toast_back() {
        mounted(|| {
            let swipe = Swipe::new();
            swipe.press(100.0);
            swipe.moved(130.0);
            assert_eq!(swipe.let_go(), LetGo::Restore);
            assert_eq!(swipe.distance(), 0.0);
        });
    }

    #[test]
    fn a_long_push_in_either_direction_dismisses_it() {
        mounted(|| {
            for target in [200.0, 0.0] {
                let swipe = Swipe::new();
                swipe.press(100.0);
                swipe.moved(target);
                assert_eq!(swipe.let_go(), LetGo::Dismiss);
            }
        });
    }

    #[test]
    fn a_gesture_taken_over_by_something_else_puts_the_toast_back() {
        mounted(|| {
            let swipe = Swipe::new();
            swipe.press(100.0);
            swipe.moved(140.0);
            swipe.cancel();
            assert_eq!(swipe.distance(), 0.0);
            assert!(!swipe.is_swiping());
            assert_eq!(swipe.let_go(), LetGo::Nothing);
        });
    }

    #[test]
    fn moving_with_no_button_down_pushes_nothing() {
        mounted(|| {
            let swipe = Swipe::new();
            assert!(!swipe.moved(400.0));
            assert_eq!(swipe.distance(), 0.0);
        });
    }
}
