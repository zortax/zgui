//! What the pointer looks like.

use smithay_client_toolkit::seat::pointer::CursorIcon;
use zgui_platform::CursorStyle;

/// The cursor a style asks for, in the shape names this desktop uses.
///
/// The names are the ones every cursor theme on this desktop ships, so a style crosses to a real
/// shape rather than to a fallback. What a theme does not have is the theme's business, and the
/// answer there is the ordinary arrow rather than an error: a missing cursor is cosmetic, and an
/// error return would be checked by nobody.
pub const fn icon(style: CursorStyle) -> Option<CursorIcon> {
    Some(match style {
        CursorStyle::None => return None,
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
        _ => CursorIcon::Default,
    })
}

#[cfg(test)]
mod tests {
    use super::icon;
    use smithay_client_toolkit::seat::pointer::CursorIcon;
    use zgui_platform::CursorStyle;

    #[test]
    fn hiding_the_pointer_is_not_a_shape() {
        // Asking a theme for a cursor called "none" gets an arrow; the request is a different one.
        assert_eq!(icon(CursorStyle::None), None);
    }

    #[test]
    fn the_ordinary_arrow_is_what_a_style_nobody_mapped_falls_back_to() {
        assert_eq!(icon(CursorStyle::Default), Some(CursorIcon::Default));
    }

    #[test]
    fn each_resize_direction_crosses_to_its_own_shape() {
        // Two directions sharing a shape is a window whose corner handle points the wrong way.
        let shapes = [
            icon(CursorStyle::ResizeEastWest),
            icon(CursorStyle::ResizeNorthSouth),
            icon(CursorStyle::ResizeNorthEastSouthWest),
            icon(CursorStyle::ResizeNorthWestSouthEast),
            icon(CursorStyle::ResizeColumn),
            icon(CursorStyle::ResizeRow),
        ];
        for (index, shape) in shapes.iter().enumerate() {
            assert!(
                !shapes[index + 1..].contains(shape),
                "{shape:?} is used for two different directions"
            );
        }
    }

    #[test]
    fn the_grab_and_the_grabbing_hand_are_different_shapes() {
        assert_ne!(icon(CursorStyle::Grab), icon(CursorStyle::Grabbing));
    }
}
