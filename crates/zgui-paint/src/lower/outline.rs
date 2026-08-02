//! `outline`, which is drawn outside the border box and takes up no space.
//!
//! Because it takes up no space, nothing about it can be recovered from a box's geometry: an
//! outline is the reason a fragment's ink can be larger than its border box even when the box casts
//! no shadow, and the reason it has to be in the ink at all.

use zgui_color::Color;
use zgui_css::parity::Support;
use zgui_css::values::border::OutlineStyleValue;
use zgui_css::values::color::{current, resolve};
use zgui_css::{ComputedStyle, register_properties};

register_properties! {
    outline_color  => Support::Implemented("zgui-paint::lower::outline"),
    outline_style  => Support::Implemented("zgui-paint::lower::outline"),
    outline_width  => Support::Implemented("zgui-paint::lower::outline"),
    outline_offset => Support::Implemented("zgui-paint::lower::outline"),
}

/// A box's outline, when it draws one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutlinePaint {
    /// The line's thickness, in device pixels.
    pub width: f32,
    /// How far outside the border box the outline's inner edge sits, in device pixels.
    ///
    /// Negative values pull the outline inwards, which CSS allows.
    pub offset: f32,
    /// The line's colour.
    pub color: Color,
    /// How the line is drawn.
    pub style: super::border::LineStyle,
}

impl OutlinePaint {
    /// How far outside the border box the outline reaches, or zero when it reaches inside it.
    pub fn reach(&self) -> f32 {
        (self.width + self.offset).max(0.0)
    }
}

/// Lowers a style's outline, or nothing when it draws none.
///
/// `outline-style: auto` is the focus ring the platform would draw; it is drawn as a solid line
/// here, which is what it looks like on every platform this runs on.
pub fn of(style: &ComputedStyle, scale: f32) -> Option<OutlinePaint> {
    let outline = style.get_outline();
    let width = outline.outline_width.0.to_f32_px() * scale;
    if outline.outline_style == OutlineStyleValue::none() || width == 0.0 {
        return None;
    }
    let color = resolve(&outline.outline_color, current(style));
    if color.alpha() == 0.0 {
        return None;
    }
    Some(OutlinePaint {
        width,
        offset: outline.outline_offset.to_f32_px() * scale,
        color,
        style: super::border::LineStyle::Solid,
    })
}

#[cfg(test)]
mod tests {
    use zgui_color::Color;

    use super::OutlinePaint;
    use crate::lower::border::LineStyle;

    /// An outline of the given width and offset.
    fn outline(width: f32, offset: f32) -> OutlinePaint {
        OutlinePaint {
            width,
            offset,
            color: Color::BLACK,
            style: LineStyle::Solid,
        }
    }

    #[test]
    fn reach_is_the_width_plus_the_offset() {
        assert_eq!(outline(2.0, 4.0).reach(), 6.0);
    }

    #[test]
    fn an_outline_pulled_inside_the_box_reaches_nowhere_outside_it() {
        assert_eq!(outline(2.0, -8.0).reach(), 0.0);
    }
}
