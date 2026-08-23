//! What this desktop turned out to be able to do.

use zgui_platform::{ClipboardFormat, DecorationSource, PlatformCapabilities};

/// What the compositor offers, built up from the empty set.
///
/// Built up rather than down, for the reason the contract gives: a capability nobody declared
/// reads as absent, which degrades, where the other direction reads as present, which breaks.
///
/// Two of the answers are decided by which globals the compositor advertised rather than by the
/// protocol having them, so they are passed in rather than assumed.
pub fn of(offered: Offered) -> PlatformCapabilities {
    let mut capabilities = PlatformCapabilities::none();
    capabilities.native_popup_surfaces = true;
    capabilities.layer_surfaces = offered.layer_shell;
    // Receiving a drop is the data device's, so it arrives with the clipboard. What arrives is
    // always paths: a drag of anything else carries something no file system can open, and this
    // backend drops it rather than turning it into a name nothing can read.
    capabilities.drop_mime_types = if offered.clipboard {
        vec!["text/uri-list".to_owned()]
    } else {
        Vec::new()
    };
    // Starting a drag towards another application is the other half of the same protocol and is
    // not spoken yet. Declared absent rather than present-and-broken: a draggable control that
    // offers itself and then never starts anything is worse than one that does not offer itself.
    capabilities.drag_source = false;
    capabilities.clipboard_formats = if offered.clipboard {
        vec![ClipboardFormat::Text]
    } else {
        Vec::new()
    };
    capabilities.clipboard_primary_selection = offered.clipboard && offered.primary_selection;
    capabilities.ime = offered.text_input;
    capabilities.ime_purpose_hints = offered.text_input;
    // A Wayland window is never told where it has been placed and can never place itself. Every
    // overlay is therefore drawn in-window or made a pop-up, and asking is how a component finds
    // that out without asking which desktop it is on.
    capabilities.absolute_window_position = false;
    // Stacking is what the layer shell is for; an ordinary window cannot ask.
    capabilities.window_levels = false;
    capabilities.decorations = if offered.server_decorations {
        DecorationSource::Platform
    } else {
        DecorationSource::Application
    };
    // The protocol for both exists and this compositor may well have it, but the contract has no
    // request that would use either: there is no method to lock a pointer and no event that would
    // carry the raw motion a lock produces. Declared absent because that is what a component
    // asking can act on — the portable backend answers the same way, for the same reason.
    capabilities.pointer_confine = false;
    capabilities.pointer_lock = false;
    // The pointer-gesture protocol is bound, but the framework's own recogniser is what the rest
    // of the tree is written against and the two must not both run.
    capabilities.native_gestures = false;
    capabilities.system_color_scheme = offered.color_scheme;
    capabilities
}

/// Which optional globals the compositor advertised.
///
/// Each is a protocol that exists and that a compositor is free not to implement, and each turns
/// into a capability rather than into a per-desktop branch anywhere above.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Offered {
    /// `zwlr_layer_shell_v1`: surfaces that are part of the desktop rather than windows.
    pub layer_shell: bool,
    /// `zxdg_decoration_manager_v1`, and a compositor willing to draw the frame itself.
    pub server_decorations: bool,
    /// Whether the data device is wired up at all.
    ///
    /// Separate from the protocol existing, because it always does: what this reports is whether
    /// this backend reads and writes it yet. A clipboard declared and unimplemented is a paste
    /// command that offers itself and always fails.
    pub clipboard: bool,
    /// `zwp_primary_selection_device_manager_v1`.
    pub primary_selection: bool,
    /// `zwp_text_input_manager_v3`.
    pub text_input: bool,
    /// A settings portal that answers the light-or-dark question.
    pub color_scheme: bool,
}

#[cfg(test)]
mod tests {
    use super::{Offered, of};
    use zgui_platform::{ClipboardFormat, DecorationSource};

    #[test]
    fn a_compositor_offering_nothing_optional_still_has_windows_and_pop_ups() {
        let capabilities = of(Offered::default());
        assert!(capabilities.native_popup_surfaces);
        assert!(!capabilities.layer_surfaces);
        assert!(!capabilities.clipboard_primary_selection);
        assert!(!capabilities.ime);
    }

    #[test]
    fn nothing_can_be_dropped_on_a_desktop_with_no_data_device() {
        assert!(!of(Offered::default()).accepts_drops());
        assert!(
            of(Offered {
                clipboard: true,
                ..Offered::default()
            })
            .accepts_drops()
        );
    }

    #[test]
    fn a_clipboard_that_is_not_wired_up_is_declared_absent_rather_than_broken() {
        // Every desktop has the protocol; what a component needs to know is whether anything
        // answers it. A paste command that offers itself and always fails is worse than none.
        assert!(!of(Offered::default()).supports_clipboard_format(ClipboardFormat::Text));
        let wired = of(Offered {
            clipboard: true,
            ..Offered::default()
        });
        assert!(wired.supports_clipboard_format(ClipboardFormat::Text));
    }

    #[test]
    fn a_window_here_never_places_itself_or_asks_to_be_stacked() {
        // Both are true of every Wayland compositor, so they are stated rather than probed. A
        // component that asks gets the same answer on the desktop that offers everything else.
        let everything = of(Offered {
            layer_shell: true,
            clipboard: true,
            server_decorations: true,
            primary_selection: true,
            text_input: true,
            color_scheme: true,
        });
        assert!(!everything.absolute_window_position);
        assert!(!everything.window_levels);
    }

    #[test]
    fn the_frame_is_drawn_by_whoever_the_compositor_left_it_to() {
        assert_eq!(
            of(Offered {
                server_decorations: true,
                ..Offered::default()
            })
            .decorations,
            DecorationSource::Platform
        );
        assert_eq!(
            of(Offered::default()).decorations,
            DecorationSource::Application
        );
    }
}
