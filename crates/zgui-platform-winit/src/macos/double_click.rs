//! What a double press on a title bar does, as the user set it in System Settings.

use objc2_foundation::{NSString, NSUserDefaults};

use crate::macos::window::ns_window;

/// What the user asked a title-bar double press to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    /// Zoom the window to fill the screen, or back.
    Zoom,
    /// Minimize the window into the dock.
    Minimize,
    /// Nothing.
    Nothing,
}

/// Reads the preference value `value` names.
fn action(value: Option<&str>) -> Action {
    match value {
        Some("Minimize") => Action::Minimize,
        Some("None") => Action::Nothing,
        _ => Action::Zoom,
    }
}

/// Does what the user asked a title-bar double press on `window` to do.
pub(crate) fn perform(window: &winit::window::Window) {
    let defaults = NSUserDefaults::standardUserDefaults();
    let value = defaults.stringForKey(&NSString::from_str("AppleActionOnDoubleClick"));
    let Some(ns_window) = ns_window(window) else {
        return;
    };
    match action(value.map(|value| value.to_string()).as_deref()) {
        Action::Zoom => ns_window.performZoom(None),
        Action::Minimize => ns_window.performMiniaturize(None),
        Action::Nothing => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, action};

    #[test]
    fn the_preference_zooms_unless_it_says_otherwise() {
        assert_eq!(action(None), Action::Zoom);
        assert_eq!(action(Some("Maximize")), Action::Zoom);
        assert_eq!(action(Some("Minimize")), Action::Minimize);
        assert_eq!(action(Some("None")), Action::Nothing);
    }
}
