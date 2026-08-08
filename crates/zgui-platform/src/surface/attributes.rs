//! What a surface is asked for before it exists.

use zgui_geom::{Css, CssPx, Point, Size};
use zgui_vocab::SharedString;

use crate::surface::chrome::{Decorations, FullscreenMode, WindowLevel};
use crate::surface::icon::WindowIcon;
use crate::theme::ColorScheme;

/// What a surface should be when it is created.
///
/// Sizes are in CSS pixels because that is the space a layout is written in; the backend applies
/// the output's scale itself.
///
/// A surface is created **hidden**. That is not a default but a rule: an accessibility adapter has
/// to be attached before the surface is first shown, and there is no second chance. The sequence a
/// backend must follow is create hidden, attach, draw one frame, then show — which is why there is
/// no "visible" attribute here at all and why [`Surface::set_visible`](crate::Surface::set_visible)
/// is the only way a surface becomes visible.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
pub struct SurfaceAttributes {
    /// The window title.
    pub title: SharedString,
    /// The size the content should start at.
    pub size: Option<Size<CssPx, Css>>,
    /// The smallest the user may make it.
    pub min_size: Option<Size<CssPx, Css>>,
    /// The largest the user may make it.
    pub max_size: Option<Size<CssPx, Css>>,
    /// Whether the user may resize it at all.
    pub resizable: bool,
    /// What frame the platform should draw.
    pub decorations: Decorations,
    /// Whether the surface may be partly transparent.
    pub transparent: bool,
    /// The identifier the desktop groups this application's windows under.
    ///
    /// Getting this wrong is what makes an application show the wrong icon in a task bar, so it is
    /// a first-class attribute rather than a platform afterthought.
    pub application_id: Option<SharedString>,
    /// Where the surface should open, measured from the desktop's origin.
    ///
    /// Ignored where a desktop places windows itself, which is every Wayland compositor.
    pub position: Option<Point<CssPx, Css>>,
    /// Whether it should open maximised.
    pub maximized: bool,
    /// Whether it should open full screen, and how.
    pub fullscreen: Option<FullscreenMode>,
    /// Where it should sit in the stacking order.
    pub level: WindowLevel,
    /// The picture the desktop should show for it, where the desktop takes one from the window.
    pub icon: Option<WindowIcon>,
    /// A light or dark preference for this surface alone; absent follows the desktop.
    pub theme: Option<ColorScheme>,
}

impl SurfaceAttributes {
    /// A resizable, decorated, opaque surface with the given title.
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            size: None,
            min_size: None,
            max_size: None,
            resizable: true,
            decorations: Decorations::Full,
            transparent: false,
            application_id: None,
            position: None,
            maximized: false,
            fullscreen: None,
            level: WindowLevel::Normal,
            icon: None,
            theme: None,
        }
    }

    /// The same attributes with a starting size.
    pub fn with_size(mut self, size: Size<CssPx, Css>) -> Self {
        self.size = Some(size);
        self
    }

    /// The same attributes with an application identifier.
    pub fn with_application_id(mut self, id: impl Into<SharedString>) -> Self {
        self.application_id = Some(id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{Decorations, SurfaceAttributes};
    use zgui_geom::{CssPx, Size};

    #[test]
    fn a_new_surface_is_resizable_and_decorated() {
        let attributes = SurfaceAttributes::new("zgui");
        assert!(attributes.resizable);
        assert_eq!(attributes.decorations, Decorations::Full);
        assert!(!attributes.transparent);
        assert_eq!(attributes.title.as_str(), "zgui");
    }

    #[test]
    fn the_builder_sets_only_what_it_names() {
        let attributes =
            SurfaceAttributes::new("zgui").with_size(Size::new(CssPx(800.0), CssPx(600.0)));
        assert_eq!(attributes.size, Some(Size::new(CssPx(800.0), CssPx(600.0))));
        assert_eq!(attributes.min_size, None);
        assert_eq!(attributes.application_id, None);
    }

    #[test]
    fn the_default_attributes_are_the_empty_ones() {
        let default = SurfaceAttributes::default();
        assert!(!default.resizable);
        assert_eq!(default.size, None);
    }
}
