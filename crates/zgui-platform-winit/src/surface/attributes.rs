//! What a window is asked for before it exists.

use winit::dpi::{LogicalPosition, LogicalSize};
use winit::window::{Fullscreen, WindowAttributes};
use zgui_platform::{ColorScheme, FullscreenMode, SurfaceAttributes};

use crate::surface::chrome;
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
    // The window's own preference wins over the desktop's, because a window that asked to be dark
    // asked for itself and not for a default to fall back on.
    let theme = attributes.theme.or(scheme).map(theme::theme);
    let mut window = WindowAttributes::default()
        .with_title(attributes.title.as_str())
        .with_resizable(attributes.resizable)
        .with_decorations(attributes.decorated)
        .with_transparent(attributes.transparent)
        .with_maximized(attributes.maximized)
        .with_window_level(chrome::level(attributes.level))
        .with_theme(theme)
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
    if let Some(position) = attributes.position {
        window = window.with_position(LogicalPosition::new(position.x.0, position.y.0));
    }
    if let Some(mode) = attributes.fullscreen {
        window = window.with_fullscreen(Some(opening_fullscreen(mode)));
    }
    if let Some(icon) = &attributes.icon {
        window = window.with_window_icon(chrome::icon(icon));
    }
    if let Some(id) = &attributes.application_id {
        window = application_name(window, id.as_str());
    }
    window
}

/// How a window that opens full screen should take the screen.
///
/// Taking it exclusively needs a video mode to take it in, and a mode belongs to the monitor the
/// window is on — which nothing knows before the window exists. A window asking for an exclusive
/// mode therefore opens borderless and is put into its exclusive mode once it has a monitor to name
/// one from, which the surface does through [`Surface::set_fullscreen`](zgui_platform::Surface::set_fullscreen).
fn opening_fullscreen(mode: FullscreenMode) -> Fullscreen {
    let _ = mode;
    Fullscreen::Borderless(None)
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
#[cfg(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
))]
fn application_name(window: WindowAttributes, id: &str) -> WindowAttributes {
    use winit::platform::wayland::WindowAttributesExtWayland;
    use winit::platform::x11::WindowAttributesExtX11;

    WindowAttributesExtX11::with_name(
        WindowAttributesExtWayland::with_name(window, id, id),
        id,
        id,
    )
}

/// The same, on a desktop that takes the application's identity from elsewhere.
///
/// Windows and macOS have no field a window carries its application's identity in. The taskbar
/// entry comes from the executable's Application User Model ID, and the Dock entry from the bundle
/// identifier in `Info.plist` — both settled before the process starts, by how the program was
/// installed rather than by what it asks for at run time. So the identifier is dropped here, and
/// dropping it costs a window nothing it could have had.
#[cfg(not(any(
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd"
)))]
fn application_name(window: WindowAttributes, id: &str) -> WindowAttributes {
    let _ = id;
    window
}

#[cfg(test)]
mod tests {
    use super::window;
    use zgui_geom::{CssPx, Point, Size};
    use zgui_platform::{ColorScheme, FullscreenMode, SurfaceAttributes, WindowLevel};

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
    ///
    /// Asserted only where there is a field to carry it. Windows and macOS take the application's
    /// identity from how the program was installed, so there is nothing in the request to read back.
    #[test]
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
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

    #[test]
    fn where_and_how_a_window_opens_reaches_the_platform() {
        let mut requested = SurfaceAttributes::new("palette");
        requested.position = Some(Point::new(CssPx(120.0), CssPx(48.0)));
        requested.maximized = true;
        requested.level = WindowLevel::AlwaysOnTop;
        requested.fullscreen = Some(FullscreenMode::Borderless);

        let attributes = window(&requested, None);
        assert!(attributes.position.is_some());
        assert!(attributes.maximized);
        assert_eq!(
            attributes.window_level,
            winit::window::WindowLevel::AlwaysOnTop
        );
        assert!(attributes.fullscreen.is_some());
    }

    #[test]
    fn a_window_that_states_a_preference_is_not_given_the_desktops() {
        // A window asking to be dark asked for itself. Letting the desktop's preference win would
        // make the attribute unusable for the one thing it exists for.
        let mut requested = SurfaceAttributes::new("zgui");
        requested.theme = Some(ColorScheme::Dark);
        let attributes = window(&requested, Some(ColorScheme::Light));
        assert_eq!(attributes.preferred_theme, Some(winit::window::Theme::Dark));

        // With no preference of its own it follows the desktop, which is what it did before.
        let following = window(&SurfaceAttributes::new("zgui"), Some(ColorScheme::Light));
        assert_eq!(
            following.preferred_theme,
            Some(winit::window::Theme::Light)
        );
    }

    #[test]
    fn an_exclusive_mode_opens_borderless_because_no_monitor_is_known_yet() {
        // A video mode belongs to the monitor the window turned out to be on, and nothing knows
        // which that is before it exists. The surface asks again once it does.
        let mut requested = SurfaceAttributes::new("player");
        requested.fullscreen = Some(FullscreenMode::Exclusive);
        let attributes = window(&requested, None);
        assert!(matches!(
            attributes.fullscreen,
            Some(winit::window::Fullscreen::Borderless(None))
        ));
    }
}
