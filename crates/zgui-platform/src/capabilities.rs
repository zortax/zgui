//! What the running platform can and cannot do.

use crate::clipboard::ClipboardFormat;
use crate::surface::DecorationSource;

/// What the platform this program is running on can actually do.
///
/// This is the escape valve that keeps per-platform knowledge out of everything above. A component
/// asks whether pop-up surfaces exist; it never asks which desktop it is on. The difference
/// matters because the first keeps working when a backend gains the feature, and the second has to
/// be found and edited.
///
/// Every field answers a question some real desktop answers differently, and each is here because
/// a component would otherwise be written against an assumption that is false somewhere.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PlatformCapabilities {
    /// Whether an overlay can be a real pop-up window of its own.
    ///
    /// On a desktop where it cannot, every menu, tooltip, dropdown and dialog is drawn inside the
    /// window that owns it and is clipped by it. That is the assumption the framework makes
    /// throughout; this flag exists so a backend that *can* do better opts in without any
    /// component changing.
    pub native_popup_surfaces: bool,
    /// Whether a surface can be a part of the desktop shell rather than a window.
    ///
    /// A wallpaper, a dock, a panel and a lock screen are all the same kind of surface and no
    /// desktop offers them by default. A program built around one checks this and says why it
    /// cannot run, rather than opening an ordinary window where a panel was meant.
    pub layer_surfaces: bool,
    /// Whether a drag can be started from this application towards another one.
    ///
    /// A draggable control written against "dragging works" is written against something several
    /// desktops do not offer at all.
    pub drag_source: bool,
    /// Which media types a drop from outside can be accepted in.
    ///
    /// Empty means nothing can be dropped on this application. A file-drop target checks this
    /// before offering itself as one, rather than showing an affordance that can never fire.
    pub drop_mime_types: Vec<String>,
    /// Which clipboard representations can be read or written at all.
    ///
    /// Plain text is on every platform. The rest are not, and a paste command offering rich text
    /// on a desktop whose clipboard backend speaks only text is a command that always fails.
    pub clipboard_formats: Vec<ClipboardFormat>,
    /// Whether the selection clipboard exists.
    pub clipboard_primary_selection: bool,
    /// Whether an input method is available and its state can be steered.
    pub ime: bool,
    /// Whether the platform accepts a hint about what kind of text a field expects.
    pub ime_purpose_hints: bool,
    /// Whether a window can read or set its own position on the desktop.
    ///
    /// Where it cannot, an overlay positioned in desktop coordinates cannot be placed at all,
    /// which is the other half of why overlays are drawn in-window.
    pub absolute_window_position: bool,
    /// Whether a window can be asked to stay above or below others.
    pub window_levels: bool,
    /// Who draws the title bar and the frame.
    pub decorations: DecorationSource,
    /// Whether the pointer can be confined to a region.
    pub pointer_confine: bool,
    /// Whether the pointer can be locked in place and read as pure motion.
    pub pointer_lock: bool,
    /// Whether the platform recognises pinch, rotate and pan gestures itself.
    ///
    /// Where it does not, the framework recognises them from the raw pointer stream instead, so
    /// this selects the source rather than the availability.
    pub native_gestures: bool,
    /// Whether the desktop's light or dark preference can be discovered.
    pub system_color_scheme: bool,
}

impl PlatformCapabilities {
    /// The capabilities of a platform that can do nothing beyond drawing and plain-text copying.
    ///
    /// This is the honest starting point for a backend to fill in, and the right answer for one
    /// that has no desktop at all. Building up from nothing means a capability a backend forgot to
    /// declare reads as absent, which degrades; building down from everything would make the same
    /// omission read as present, which breaks.
    pub fn none() -> Self {
        Self {
            native_popup_surfaces: false,
            layer_surfaces: false,
            drag_source: false,
            drop_mime_types: Vec::new(),
            clipboard_formats: vec![ClipboardFormat::Text],
            clipboard_primary_selection: false,
            ime: false,
            ime_purpose_hints: false,
            absolute_window_position: false,
            window_levels: false,
            decorations: DecorationSource::Platform,
            pointer_confine: false,
            pointer_lock: false,
            native_gestures: false,
            system_color_scheme: false,
        }
    }

    /// Whether a clipboard representation can be used at all.
    ///
    /// ```
    /// use zgui_platform::{ClipboardFormat, PlatformCapabilities};
    ///
    /// let capabilities = PlatformCapabilities::none();
    /// assert!(capabilities.supports_clipboard_format(ClipboardFormat::Text));
    /// assert!(!capabilities.supports_clipboard_format(ClipboardFormat::Image));
    /// ```
    pub fn supports_clipboard_format(&self, format: ClipboardFormat) -> bool {
        self.clipboard_formats.contains(&format)
    }

    /// Whether anything at all can be dropped on this application from outside.
    pub fn accepts_drops(&self) -> bool {
        !self.drop_mime_types.is_empty()
    }
}

impl Default for PlatformCapabilities {
    fn default() -> Self {
        Self::none()
    }
}

#[cfg(test)]
mod tests {
    use super::PlatformCapabilities;
    use crate::clipboard::ClipboardFormat;
    use crate::surface::DecorationSource;

    #[test]
    fn the_starting_point_claims_nothing_but_plain_text() {
        let capabilities = PlatformCapabilities::none();
        assert!(!capabilities.native_popup_surfaces);
        assert!(!capabilities.layer_surfaces);
        assert!(!capabilities.drag_source);
        assert!(!capabilities.accepts_drops());
        assert!(!capabilities.ime);
        assert!(!capabilities.system_color_scheme);
        assert_eq!(capabilities.decorations, DecorationSource::Platform);
        assert_eq!(capabilities.clipboard_formats, [ClipboardFormat::Text]);
    }

    #[test]
    fn a_backend_declares_capabilities_by_adding_to_the_empty_set() {
        let mut capabilities = PlatformCapabilities::none();
        capabilities
            .drop_mime_types
            .push("text/uri-list".to_owned());
        capabilities.clipboard_formats.push(ClipboardFormat::Html);
        assert!(capabilities.accepts_drops());
        assert!(capabilities.supports_clipboard_format(ClipboardFormat::Html));
        assert!(!capabilities.supports_clipboard_format(ClipboardFormat::Image));
    }

    #[test]
    fn the_default_is_the_empty_set() {
        assert_eq!(
            PlatformCapabilities::default(),
            PlatformCapabilities::none()
        );
    }
}
