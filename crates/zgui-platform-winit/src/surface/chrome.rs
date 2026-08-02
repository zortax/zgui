//! The window's own furniture: cursors, resize edges, full screen.

use winit::window::{CursorIcon, Fullscreen, ResizeDirection, Window};
use zgui_platform::{CursorStyle, FullscreenMode, ResizeEdge};

/// What the pointer should look like.
///
/// A style the desktop has no cursor for falls back to the ordinary arrow rather than failing,
/// because a missing cursor is cosmetic and an error nobody would check is worse than a wrong
/// arrow.
pub(crate) const fn cursor(style: CursorStyle) -> Option<CursorIcon> {
    Some(match style {
        CursorStyle::Default => CursorIcon::Default,
        CursorStyle::Pointer => CursorIcon::Pointer,
        CursorStyle::Text => CursorIcon::Text,
        CursorStyle::VerticalText => CursorIcon::VerticalText,
        CursorStyle::Crosshair => CursorIcon::Crosshair,
        CursorStyle::Grab => CursorIcon::Grab,
        CursorStyle::Grabbing => CursorIcon::Grabbing,
        CursorStyle::Wait => CursorIcon::Wait,
        CursorStyle::Progress => CursorIcon::Progress,
        CursorStyle::NotAllowed => CursorIcon::NotAllowed,
        CursorStyle::Move => CursorIcon::Move,
        CursorStyle::ResizeColumn => CursorIcon::ColResize,
        CursorStyle::ResizeRow => CursorIcon::RowResize,
        CursorStyle::ResizeEastWest => CursorIcon::EwResize,
        CursorStyle::ResizeNorthSouth => CursorIcon::NsResize,
        CursorStyle::ResizeNorthEastSouthWest => CursorIcon::NeswResize,
        CursorStyle::ResizeNorthWestSouthEast => CursorIcon::NwseResize,
        // Hiding the pointer is not a cursor at all; the caller stops drawing one instead.
        CursorStyle::None => return None,
        _ => CursorIcon::Default,
    })
}

/// Which edge or corner a resize was started from.
pub(crate) const fn resize(edge: ResizeEdge) -> ResizeDirection {
    match edge {
        ResizeEdge::North => ResizeDirection::North,
        ResizeEdge::South => ResizeDirection::South,
        ResizeEdge::East => ResizeDirection::East,
        ResizeEdge::West => ResizeDirection::West,
        ResizeEdge::NorthEast => ResizeDirection::NorthEast,
        ResizeEdge::NorthWest => ResizeDirection::NorthWest,
        ResizeEdge::SouthEast => ResizeDirection::SouthEast,
        _ => ResizeDirection::SouthWest,
    }
}

/// How a window should fill the screen.
///
/// Taking the screen exclusively needs a display mode to take it in, and a window that is not on
/// any output cannot name one. Where no mode can be found the window fills the screen without
/// changing its mode instead, which is what the user asked for minus the part that could not be
/// done — rather than nothing, which is what refusing would give them.
pub(crate) fn fullscreen(window: &Window, mode: FullscreenMode) -> Fullscreen {
    match mode {
        FullscreenMode::Exclusive => window
            .current_monitor()
            .and_then(|monitor| monitor.video_modes().next())
            .map_or(Fullscreen::Borderless(None), Fullscreen::Exclusive),
        _ => Fullscreen::Borderless(None),
    }
}

#[cfg(test)]
mod tests {
    use super::{cursor, resize};
    use winit::window::{CursorIcon, ResizeDirection};
    use zgui_platform::{CursorStyle, ResizeEdge};

    #[test]
    fn hiding_the_pointer_is_not_a_cursor() {
        // The one style with no icon behind it. Answering with the arrow instead would leave a
        // pointer visible over a video player that had asked for it to go away.
        assert_eq!(cursor(CursorStyle::None), None);
        assert_eq!(cursor(CursorStyle::Default), Some(CursorIcon::Default));
    }

    #[test]
    fn the_styles_that_say_what_will_happen_never_collapse_into_each_other() {
        // A resize cursor that pointed the wrong way, or a "will not accept this" cursor that
        // looked like an ordinary arrow, is an interface lying about what a drag will do.
        let distinct = [
            CursorStyle::Pointer,
            CursorStyle::Text,
            CursorStyle::Grab,
            CursorStyle::Grabbing,
            CursorStyle::NotAllowed,
            CursorStyle::ResizeEastWest,
            CursorStyle::ResizeNorthSouth,
            CursorStyle::ResizeNorthEastSouthWest,
            CursorStyle::ResizeNorthWestSouthEast,
        ];
        for (index, style) in distinct.iter().enumerate() {
            for other in &distinct[index + 1..] {
                assert_ne!(
                    cursor(*style),
                    cursor(*other),
                    "{style:?} looks like {other:?}"
                );
            }
        }
    }

    #[test]
    fn every_edge_crosses_to_its_own_edge() {
        let pairs = [
            (ResizeEdge::North, ResizeDirection::North),
            (ResizeEdge::South, ResizeDirection::South),
            (ResizeEdge::East, ResizeDirection::East),
            (ResizeEdge::West, ResizeDirection::West),
            (ResizeEdge::NorthEast, ResizeDirection::NorthEast),
            (ResizeEdge::NorthWest, ResizeDirection::NorthWest),
            (ResizeEdge::SouthEast, ResizeDirection::SouthEast),
            (ResizeEdge::SouthWest, ResizeDirection::SouthWest),
        ];
        for (edge, direction) in pairs {
            assert_eq!(resize(edge), direction, "{edge:?} crossed wrongly");
        }
    }
}
