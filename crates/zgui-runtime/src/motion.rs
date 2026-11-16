//! Whether the application is styled for reduced motion.
//!
//! The preference is the input to the `prefers-reduced-motion` media feature in every window. The
//! application can set its own value. Without one, the desktop's preference decides, and a desktop
//! that cannot be asked states no preference.
//!
//! ```no_run
//! use zgui_runtime::motion::use_reduced_motion;
//! use zgui_reactive::prelude::*;
//!
//! # fn example() {
//! let motion = use_reduced_motion();
//! // A setting in the application overrides the desktop.
//! motion.set(Some(true));
//! assert!(motion.reduced().get_untracked());
//! // `None` follows the desktop again.
//! motion.set(None);
//! # }
//! ```

use zgui_reactive::prelude::*;
use zgui_reactive::{ArcRwSignal, Signal};

/// The reduced-motion preference of the running application.
///
/// Provided above every window, so every window and the application's own scope see the same
/// value. Cheap to clone.
#[derive(Clone)]
pub struct ReducedMotion {
    /// The application's own value, when it set one.
    app: ArcRwSignal<Option<bool>>,
    /// The desktop's preference, as the platform last reported it.
    desktop: ArcRwSignal<Option<bool>>,
}

impl ReducedMotion {
    /// A preference with no application value and an unknown desktop.
    pub(crate) fn new() -> Self {
        Self {
            app: ArcRwSignal::new(None),
            desktop: ArcRwSignal::new(None),
        }
    }

    /// Whether `prefers-reduced-motion` answers `reduce`, as a signal.
    ///
    /// The application's value when it set one, else the desktop's preference, else `false`.
    pub fn reduced(&self) -> Signal<bool> {
        let app = self.app.clone();
        let desktop = self.desktop.clone();
        Signal::derive(move || resolve(app.get(), desktop.get()))
    }

    /// The desktop's preference, or `None` when the platform cannot report it.
    pub fn desktop(&self) -> Signal<Option<bool>> {
        self.desktop.clone().into()
    }

    /// Sets the application's own value. `None` follows the desktop.
    ///
    /// Every window restyles on its next frame when the answer moves.
    pub fn set(&self, reduce: Option<bool>) {
        if self.app.get_untracked() != reduce {
            self.app.set(reduce);
        }
    }

    /// Records the desktop's preference, and reports whether it moved.
    pub(crate) fn note_desktop(&self, reduce: Option<bool>) -> bool {
        let moved = self.desktop.get_untracked() != reduce;
        if moved {
            self.desktop.set(reduce);
        }
        moved
    }

    /// The answer now, without tracking.
    pub(crate) fn current(&self) -> bool {
        resolve(self.app.get_untracked(), self.desktop.get_untracked())
    }
}

/// The answer for an application value and a desktop preference.
fn resolve(app: Option<bool>, desktop: Option<bool>) -> bool {
    app.or(desktop).unwrap_or(false)
}

/// The reduced-motion preference of the running application.
///
/// # Panics
///
/// Panics when called outside a running application. Use [`try_use_reduced_motion`] where code
/// can run outside one.
pub fn use_reduced_motion() -> ReducedMotion {
    try_use_reduced_motion().expect("this code is not running inside an application")
}

/// The reduced-motion preference of the running application, if one is running.
pub fn try_use_reduced_motion() -> Option<ReducedMotion> {
    zgui_reactive::use_local_context::<ReducedMotion>()
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn the_application_value_wins_over_the_desktop() {
        assert!(!resolve(None, None), "no preference is the default");
        assert!(resolve(None, Some(true)));
        assert!(!resolve(Some(false), Some(true)));
        assert!(resolve(Some(true), Some(false)));
    }
}
