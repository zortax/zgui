//! What a window is asked for before it exists.

use zgui_geom::{CssPx, Point, Size};
use zgui_platform::{
    ColorScheme, FullscreenMode, SurfaceAttributes, WindowIcon, WindowLevel,
};
use zgui_vocab::SharedString;

use crate::commands::CloseResponse;
use crate::window::WindowContent;

/// Everything a window can be asked for.
///
/// Asking for something a desktop cannot do is not an error and needs no branch in the application:
/// the request is simply not carried out. A window that asked to open at a point on a compositor
/// that places windows itself opens where the compositor put it.
#[derive(Default)]
pub struct WindowOptions {
    /// What the surface should be.
    pub(crate) attributes: SurfaceAttributes,
    /// What the window should hold.
    pub(crate) runtime: WindowContent,
    /// What to ask before a close the user asked for is carried out.
    pub(crate) on_close_request: Option<Box<dyn FnMut() -> CloseResponse>>,
}

impl WindowOptions {
    /// A resizable, decorated window with the given title.
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            attributes: SurfaceAttributes::new(title),
            runtime: WindowContent::default(),
            on_close_request: None,
        }
    }

    /// Names the window. This is what a user reads, and it changes with the document.
    pub fn with_title(mut self, title: impl Into<SharedString>) -> Self {
        self.attributes.title = title.into();
        self
    }

    /// The size the content should start at, in CSS pixels.
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.attributes.size = Some(Size::new(CssPx(width), CssPx(height)));
        self
    }

    /// The smallest the user may make it, in CSS pixels.
    pub fn with_min_size(mut self, width: f32, height: f32) -> Self {
        self.attributes.min_size = Some(Size::new(CssPx(width), CssPx(height)));
        self
    }

    /// The largest the user may make it, in CSS pixels.
    pub fn with_max_size(mut self, width: f32, height: f32) -> Self {
        self.attributes.max_size = Some(Size::new(CssPx(width), CssPx(height)));
        self
    }

    /// Whether the user may resize it at all.
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.attributes.resizable = resizable;
        self
    }

    /// Whether the desktop should draw a title bar and a frame.
    ///
    /// A window that turns this off draws its own, and owes the user the affordances the desktop
    /// would have given them: something to drag it by
    /// ([`WindowHandle::move_drag_handler`](crate::windows::WindowHandle::move_drag_handler)),
    /// edges to resize from, and a way to close it.
    pub fn with_decorations(mut self, decorated: bool) -> Self {
        self.attributes.decorated = decorated;
        self
    }

    /// Whether the window may be partly transparent.
    ///
    /// What a window with rounded corners of its own needs, so the desktop shows through them.
    pub fn with_transparent(mut self, transparent: bool) -> Self {
        self.attributes.transparent = transparent;
        self
    }

    /// Where it should open, measured from the desktop's origin.
    ///
    /// Ignored where a desktop places windows itself, which is every Wayland compositor.
    pub fn with_position(mut self, x: f32, y: f32) -> Self {
        self.attributes.position = Some(Point::new(CssPx(x), CssPx(y)));
        self
    }

    /// Whether it should open maximised.
    pub fn with_maximized(mut self, maximized: bool) -> Self {
        self.attributes.maximized = maximized;
        self
    }

    /// Whether it should open full screen, and how.
    pub fn with_fullscreen(mut self, mode: Option<FullscreenMode>) -> Self {
        self.attributes.fullscreen = mode;
        self
    }

    /// Where it should sit in the desktop's stacking order.
    pub fn with_level(mut self, level: WindowLevel) -> Self {
        self.attributes.level = level;
        self
    }

    /// The picture the desktop should show for it.
    pub fn with_icon(mut self, icon: WindowIcon) -> Self {
        self.attributes.icon = Some(icon);
        self
    }

    /// A light or dark preference for this window alone.
    pub fn with_theme(mut self, theme: ColorScheme) -> Self {
        self.attributes.theme = Some(theme);
        self
    }

    /// This window's own stylesheet, cascaded after the application's.
    pub fn with_stylesheet(mut self, css: impl Into<String>) -> Self {
        self.runtime.window_stylesheet = Some(css.into());
        self
    }

    /// Asks `callback` before a close the user asked for is carried out.
    ///
    /// Answering [`CloseResponse::Veto`] keeps the window: what a document with unsaved work does
    /// while it asks. Whatever refuses owes the user another way out, because a window that always
    /// refuses cannot be closed at all.
    pub fn on_close_request(
        mut self,
        callback: impl FnMut() -> CloseResponse + 'static,
    ) -> Self {
        self.on_close_request = Some(Box::new(callback));
        self
    }

    /// The surface attributes this asks for, for a caller assembling them itself.
    pub fn attributes(&self) -> &SurfaceAttributes {
        &self.attributes
    }
}

impl core::fmt::Debug for WindowOptions {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("WindowOptions")
            .field("attributes", &self.attributes)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::WindowOptions;
    use zgui_geom::CssPx;

    #[test]
    fn the_builder_sets_only_what_it_names() {
        let options = WindowOptions::new("settings").with_size(480.0, 600.0);
        assert_eq!(options.attributes().title.as_str(), "settings");
        assert_eq!(
            options.attributes().size.map(|size| size.width),
            Some(CssPx(480.0))
        );
        assert!(options.attributes().resizable, "the default is resizable");
        assert!(options.attributes().decorated);
        assert_eq!(options.attributes().position, None);
    }

    #[test]
    fn a_window_that_draws_its_own_frame_says_so_in_one_place() {
        let options = WindowOptions::new("csd")
            .with_decorations(false)
            .with_transparent(true);
        assert!(!options.attributes().decorated);
        assert!(options.attributes().transparent);
    }
}
