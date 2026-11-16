//! Whether the window's document is styled for reduced motion.
//!
//! The preference is the input to `prefers-reduced-motion`. It lives in the viewport the cascade's
//! device is built from, so a change reaches the media queries on the next frame. The runtime sets
//! it from the application's own value, or from the desktop's preference when the application
//! sets none.

use crate::window::Window;

impl Window {
    /// Whether `prefers-reduced-motion` answers `reduce` in this window.
    pub fn reduced_motion(&self) -> bool {
        self.viewport.reduced_motion
    }

    /// Styles this window for reduced motion when `reduce` is set, and reports whether that moved.
    ///
    /// A move rebuilds the cascade's device on the next frame and re-matches the rules of every
    /// origin whose media queries read the preference.
    pub fn set_reduced_motion(&mut self, reduce: bool) -> bool {
        if self.viewport.reduced_motion == reduce {
            return false;
        }
        self.viewport = self.viewport.with_reduced_motion(reduce);
        true
    }
}
