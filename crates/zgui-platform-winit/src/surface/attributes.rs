//! What a window is asked for before it exists.

use winit::dpi::LogicalSize;
use winit::platform::wayland::WindowAttributesExtWayland;
use winit::platform::x11::WindowAttributesExtX11;
use winit::window::WindowAttributes;
use zgui_platform::{ColorScheme, SurfaceAttributes};

use crate::theme;

/// The window attributes a surface request stands for.
///
/// **The window is always created hidden**, whatever else was asked for, and that is a rule rather
/// than a default. An accessibility adapter has to be attached before a window is shown for the
/// first time and there is no second chance at it — the platform adapter refuses outright — so the
/// only order that works is create hidden, attach, draw one frame, then show. Showing it here
/// would also produce the flash of empty window at launch that this ordering exists to avoid.
///
/// Sizes cross as *logical* sizes, which is the platform's name for the space a layout is written
/// in. Handing physical pixels over instead would make a window on a doubled display come out
/// twice the size it was asked for.
pub(crate) fn window(
    attributes: &SurfaceAttributes,
    scheme: Option<ColorScheme>,
) -> WindowAttributes {
    let mut window = WindowAttributes::default()
        .with_title(attributes.title.as_str())
        .with_resizable(attributes.resizable)
        .with_decorations(attributes.decorated)
        .with_transparent(attributes.transparent)
        .with_theme(scheme.map(theme::theme))
        .with_visible(false);
    if let Some(size) = attributes.size {
        window = window.with_inner_size(LogicalSize::new(size.width.0, size.height.0));
    }
    if let Some(size) = attributes.min_size {
        window = window.with_min_inner_size(LogicalSize::new(size.width.0, size.height.0));
    }
    if let Some(size) = attributes.max_size {
        window = window.with_max_inner_size(LogicalSize::new(size.width.0, size.height.0));
    }
    if let Some(id) = &attributes.application_id {
        window = application_name(window, id.as_str());
    }
    window
}

/// Names the application to whichever display server the window ends up on.
///
/// Which one that is cannot be known here — the backend is chosen when the event loop is built, and
/// the same binary is expected to run under a Wayland compositor and under X11 — so the name is set
/// through both display servers' extensions. On Wayland it becomes the toplevel's `app_id`; on X11
/// it becomes the general class of `WM_CLASS`, with the instance part beside it. A window with
/// neither is one the desktop cannot address at all: no icon, no window rules, no task-bar grouping.
///
/// The two extensions currently write one field, so the second call restates the first rather than
/// adding to it. Both are made anyway, because which of the two a window request carries is the
/// windowing layer's business and not this function's, and a request that named only one of them
/// would be silently wrong the day they part company.
///
/// The instance name is the identifier again rather than something derived from the executable's
/// path: the instance distinguishes two windows of one program from each other, this backend opens
/// windows that are not distinguished that way, and a name taken from `argv[0]` would change
/// depending on how the program was invoked.
fn application_name(window: WindowAttributes, id: &str) -> WindowAttributes {
    WindowAttributesExtX11::with_name(
        WindowAttributesExtWayland::with_name(window, id, id),
        id,
        id,
    )
}

#[cfg(test)]
mod tests {
    use super::window;
    use zgui_geom::{CssPx, Size};
    use zgui_platform::SurfaceAttributes;

    #[test]
    fn a_window_is_always_asked_for_hidden_however_it_was_requested() {
        // The accessibility adapter cannot be attached to a window that has already been shown, so
        // a visible-on-creation window is an application with no accessibility at all.
        let attributes = window(&SurfaceAttributes::new("zgui"), None);
        assert!(!attributes.visible);
    }

    #[test]
    fn what_was_asked_for_is_what_is_asked_of_the_platform() {
        let mut requested =
            SurfaceAttributes::new("counter").with_size(Size::new(CssPx(480.0), CssPx(320.0)));
        requested.decorated = false;
        requested.resizable = false;
        requested.transparent = true;

        let attributes = window(&requested, None);
        assert_eq!(attributes.title, "counter");
        assert!(!attributes.decorations);
        assert!(!attributes.resizable);
        assert!(attributes.transparent);
        assert!(attributes.inner_size.is_some());
    }

    /// The application identifier reaches the window request, on both display servers.
    ///
    /// It is read back out of the request's own rendering because winit keeps the platform-specific
    /// half of a window request private, and a test that only called the setter would pass whether
    /// or not this function ever called it — which is exactly how the identifier came to be an
    /// attribute nothing consumed. The negative control is the other half: the same request without
    /// an identifier must not carry one, or the assertion below would hold for a constant.
    #[test]
    fn an_application_identifier_reaches_the_window_request() {
        let named = window(
            &SurfaceAttributes::new("counter").with_application_id("dev.zgui.Counter"),
            None,
        );
        let rendered = format!("{named:?}");
        assert!(
            rendered.contains("dev.zgui.Counter"),
            "the window request carries no application name: {rendered}"
        );

        let anonymous = format!("{:?}", window(&SurfaceAttributes::new("counter"), None));
        assert!(
            !anonymous.contains("dev.zgui.Counter"),
            "an unnamed request must not carry a name, or the case above proves nothing"
        );
        assert!(
            anonymous.contains("name: None"),
            "an unnamed request is the one the desktop cannot address: {anonymous}"
        );
    }

    #[test]
    fn the_bounds_a_user_may_drag_to_are_absent_until_they_are_asked_for() {
        let attributes = window(&SurfaceAttributes::new("zgui"), None);
        assert!(attributes.min_inner_size.is_none());
        assert!(attributes.max_inner_size.is_none());
    }
}
