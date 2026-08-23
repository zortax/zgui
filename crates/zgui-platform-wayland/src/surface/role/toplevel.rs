//! An ordinary window: what the compositor is asked for and what it answers.

use std::num::NonZeroU32;

use smithay_client_toolkit::reexports::csd_frame::WindowState;
use smithay_client_toolkit::shell::xdg::window::{DecorationMode, WindowDecorations};
use zgui_geom::{Css, CssPx, Size};
use zgui_platform::{DecorationSource, Decorations};

/// The decoration the compositor is asked for.
///
/// Asking the server to draw the frame is the default and is what a desktop expects: a window
/// whose title bar matches every other window's. `None` and `NoTitleBar` both mean the application
/// draws its own furniture, so the compositor is told to draw none — a compositor that refuses and
/// draws one anyway would leave two title bars stacked.
pub const fn decorations(decorations: Decorations) -> WindowDecorations {
    match decorations {
        Decorations::Full => WindowDecorations::RequestServer,
        _ => WindowDecorations::RequestClient,
    }
}

/// Who is drawing the frame, as the compositor settled it.
pub const fn source(mode: DecorationMode) -> DecorationSource {
    match mode {
        DecorationMode::Server => DecorationSource::Platform,
        DecorationMode::Client => DecorationSource::Application,
    }
}

/// The extent a configure settles on, given what the surface is at now.
///
/// A compositor answers the first configure with no size at all, which means "whatever you like":
/// the answer is then what the application asked for. Later configures during a drag do name one,
/// and a named extent is not a suggestion.
pub fn extent(
    named: (Option<NonZeroU32>, Option<NonZeroU32>),
    wanted: Size<CssPx, Css>,
    current: Size<CssPx, Css>,
) -> Size<CssPx, Css> {
    let fallback = if current.width.0 > 0.0 && current.height.0 > 0.0 {
        current
    } else {
        wanted
    };
    let width = named
        .0
        .map_or(fallback.width, |width| CssPx(width.get() as f32));
    let height = named
        .1
        .map_or(fallback.height, |height| CssPx(height.get() as f32));
    Size::new(width, height)
}

/// Whether the compositor said it has stopped repainting this window.
///
/// The state arrived in version 6 of the shell. On an older compositor it is never set, and
/// visibility falls back to the outputs the surface is on.
pub fn suspended(state: WindowState) -> bool {
    state.contains(WindowState::SUSPENDED)
}

#[cfg(test)]
mod tests {
    use super::{decorations, extent, source, suspended};
    use smithay_client_toolkit::reexports::csd_frame::WindowState;
    use smithay_client_toolkit::shell::xdg::window::{DecorationMode, WindowDecorations};
    use std::num::NonZeroU32;
    use zgui_geom::{Css, CssPx, Size};
    use zgui_platform::{DecorationSource, Decorations};

    fn named(size: (Option<u32>, Option<u32>)) -> (Option<NonZeroU32>, Option<NonZeroU32>) {
        (
            size.0.and_then(NonZeroU32::new),
            size.1.and_then(NonZeroU32::new),
        )
    }

    fn size(width: f32, height: f32) -> Size<CssPx, Css> {
        Size::new(CssPx(width), CssPx(height))
    }

    #[test]
    fn a_window_that_draws_its_own_furniture_asks_the_compositor_for_none() {
        assert_eq!(
            decorations(Decorations::Full),
            WindowDecorations::RequestServer
        );
        assert_eq!(
            decorations(Decorations::None),
            WindowDecorations::RequestClient
        );
        assert_eq!(
            decorations(Decorations::NoTitleBar),
            WindowDecorations::RequestClient
        );
    }

    #[test]
    fn a_first_configure_naming_no_size_takes_the_one_that_was_asked_for() {
        let taken = extent(named((None, None)), size(800.0, 600.0), size(0.0, 0.0));
        assert_eq!(taken, size(800.0, 600.0));
    }

    #[test]
    fn a_later_configure_naming_no_size_keeps_the_extent_the_window_is_at() {
        // A compositor re-states a window's own size as "whatever you like" whenever nothing about
        // it changed. Taking the launch size there would snap a resized window back on every ping.
        let taken = extent(named((None, None)), size(800.0, 600.0), size(1024.0, 768.0));
        assert_eq!(taken, size(1024.0, 768.0));
    }

    #[test]
    fn a_named_extent_is_taken_on_each_axis_the_compositor_named() {
        let taken = extent(
            named((Some(640), None)),
            size(800.0, 600.0),
            size(1024.0, 768.0),
        );
        assert_eq!(taken, size(640.0, 768.0));
    }

    #[test]
    fn the_frame_is_attributed_to_whoever_the_compositor_said_draws_it() {
        assert_eq!(source(DecorationMode::Server), DecorationSource::Platform);
        assert_eq!(
            source(DecorationMode::Client),
            DecorationSource::Application
        );
    }

    #[test]
    fn a_window_is_suspended_only_when_the_compositor_says_so() {
        assert!(!suspended(WindowState::empty()));
        assert!(!suspended(WindowState::MAXIMIZED));
        assert!(suspended(WindowState::SUSPENDED | WindowState::ACTIVATED));
    }
}
