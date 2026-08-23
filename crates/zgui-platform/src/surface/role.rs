//! What a surface is to the desktop: an ordinary window, a pop-up, or a shell layer.

use zgui_geom::{Css, CssPx, Edges, Rect};

use crate::surface::id::SurfaceId;

/// What a surface is to the desktop.
///
/// A backend that cannot make a role refuses the surface with
/// [`PlatformError::Unsupported`](crate::PlatformError::Unsupported) rather than making an
/// ordinary window instead, because a menu silently opening as a second application window is
/// worse than a menu that does not open. Whether a role is available at all is
/// [`PlatformCapabilities`](crate::PlatformCapabilities)' to answer, and a component asks that
/// before it asks for the surface.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub enum SurfaceRole {
    /// An ordinary window.
    #[default]
    Toplevel,
    /// A surface placed against a rectangle of another surface, and dismissed with it.
    Popup(PopupPlacement),
    /// A part of the desktop shell: a wallpaper, a dock, a notification, a lock screen.
    Layer(LayerPlacement),
}

/// Where a pop-up sits relative to the surface that owns it.
///
/// The rectangle and both corners are the same model X11, Wayland and every menu library agree on:
/// a rectangle on the parent, a corner of that rectangle the pop-up is anchored to, and the corner
/// of the pop-up that meets it. What happens when the result would leave the screen is
/// [`Constrain`]'s.
#[derive(Clone, Debug, PartialEq)]
pub struct PopupPlacement {
    /// The surface the pop-up belongs to and is measured against.
    pub parent: SurfaceId,
    /// The rectangle on the parent to anchor against, in the parent's own coordinates.
    pub anchor_rect: Rect<CssPx, Css>,
    /// Which corner of the anchor rectangle the pop-up hangs from.
    pub anchor: Anchor,
    /// Which corner of the pop-up meets it.
    pub gravity: Anchor,
    /// What to do when the pop-up would not fit.
    pub constrain: Constrain,
    /// Whether the pop-up takes the pointer and keyboard until it is dismissed.
    ///
    /// A menu does; a tooltip does not. A grab is only granted against a press the user made, so a
    /// pop-up asking for one without a preceding press is dismissed by the desktop immediately.
    pub grab: bool,
}

impl PopupPlacement {
    /// A grabbing pop-up under `anchor_rect`, aligned to its left, sliding to stay on screen.
    pub fn below(parent: SurfaceId, anchor_rect: Rect<CssPx, Css>) -> Self {
        Self {
            parent,
            anchor_rect,
            anchor: Anchor::BottomLeft,
            gravity: Anchor::BottomRight,
            constrain: Constrain::SLIDE,
            grab: true,
        }
    }
}

/// A corner, an edge or the middle of a rectangle.
///
/// As an anchor it names the point on the parent's rectangle a pop-up hangs from. As a gravity it
/// names the direction the pop-up extends in: [`Anchor::BottomRight`] puts the pop-up below and to
/// the right of the anchor point, which is where a left-aligned drop-down goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Anchor {
    /// The middle.
    #[default]
    Center,
    /// The middle of the top edge.
    Top,
    /// The middle of the bottom edge.
    Bottom,
    /// The middle of the left edge.
    Left,
    /// The middle of the right edge.
    Right,
    /// The top-left corner.
    TopLeft,
    /// The top-right corner.
    TopRight,
    /// The bottom-left corner.
    BottomLeft,
    /// The bottom-right corner.
    BottomRight,
}

/// What a desktop may do to a pop-up that would not fit on the screen.
///
/// Each is allowed rather than requested: the desktop applies the ones it is given, in whatever
/// order it prefers, and only as far as it has to. Granting none of them leaves a menu hanging off
/// the edge of the screen with its items unreachable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Constrain {
    /// Move the pop-up along the axis until it fits.
    pub slide: bool,
    /// Reflect it to the other side of the anchor.
    pub flip: bool,
    /// Shrink it.
    pub resize: bool,
}

impl Constrain {
    /// Nothing: the pop-up appears exactly where it was asked for, on screen or not.
    pub const NONE: Self = Self {
        slide: false,
        flip: false,
        resize: false,
    };
    /// Sliding only, which is what a drop-down under a field wants.
    pub const SLIDE: Self = Self {
        slide: true,
        flip: false,
        resize: false,
    };
    /// Everything, which is what a nested menu wants.
    pub const ANY: Self = Self {
        slide: true,
        flip: true,
        resize: true,
    };
}

/// Where a shell layer sits and how much of the screen it claims.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerPlacement {
    /// Which layer of the desktop it belongs to.
    pub layer: Layer,
    /// Which screen edges it is fastened to.
    ///
    /// Two opposite edges stretch the surface between them and its size on that axis is ignored.
    /// No edges at all centres it.
    pub anchors: Edges<bool>,
    /// How far in from each anchored edge it sits.
    pub margin: Edges<CssPx>,
    /// How much room to reserve for it, so that ordinary windows do not open underneath.
    ///
    /// Absent lets the desktop take the surface's own extent along its anchored edge, which is what
    /// a dock wants. Zero reserves nothing, which is what an overlay wants.
    pub exclusive_zone: Option<CssPx>,
    /// Whether the surface takes the keyboard.
    pub keyboard: KeyboardInteractivity,
    /// The output to place it on, named as [`MonitorInfo::name`](crate::MonitorInfo::name)
    /// reports it. Absent lets the desktop choose.
    pub monitor: Option<String>,
}

/// Which layer of the desktop a shell surface belongs to.
///
/// The order is the stacking order, bottom first. Ordinary windows sit between
/// [`Layer::Bottom`] and [`Layer::Top`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Layer {
    /// Behind everything: a wallpaper.
    Background,
    /// Below ordinary windows: a desktop widget.
    Bottom,
    /// Above ordinary windows: a dock, a panel, a bar.
    #[default]
    Top,
    /// Above everything: a notification, a launcher, a lock screen.
    Overlay,
}

/// How much of the keyboard a shell layer takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyboardInteractivity {
    /// None at all; the surface is decoration and the keyboard goes to the window underneath.
    #[default]
    None,
    /// All of it, for as long as the surface exists. A lock screen asks for this.
    Exclusive,
    /// Only while the user is interacting with it. A launcher asks for this.
    OnDemand,
}

#[cfg(test)]
mod tests {
    use super::{Anchor, Constrain, KeyboardInteractivity, Layer, PopupPlacement, SurfaceRole};
    use crate::surface::id::SurfaceId;
    use zgui_geom::{CssPx, Point, Rect, Size};

    #[test]
    fn a_surface_that_names_no_role_is_an_ordinary_window() {
        assert_eq!(SurfaceRole::default(), SurfaceRole::Toplevel);
    }

    #[test]
    fn a_drop_down_hangs_from_the_bottom_left_of_its_field_and_slides() {
        let rect = Rect::new(
            Point::new(CssPx(4.0), CssPx(8.0)),
            Size::new(CssPx(120.0), CssPx(24.0)),
        );
        let placement = PopupPlacement::below(SurfaceId::new(1), rect);
        assert_eq!(placement.anchor, Anchor::BottomLeft);
        assert_eq!(placement.gravity, Anchor::BottomRight);
        assert_eq!(placement.constrain, Constrain::SLIDE);
        assert!(placement.grab);
    }

    #[test]
    fn the_layers_stack_from_the_wallpaper_up() {
        assert!(Layer::Background < Layer::Bottom);
        assert!(Layer::Bottom < Layer::Top);
        assert!(Layer::Top < Layer::Overlay);
    }

    #[test]
    fn a_layer_takes_no_keyboard_unless_it_asks() {
        assert_eq!(
            KeyboardInteractivity::default(),
            KeyboardInteractivity::None
        );
    }

    #[test]
    fn constraining_nothing_is_not_the_same_as_constraining_everything() {
        const { assert!(!Constrain::NONE.slide) };
        assert_eq!(Constrain::default(), Constrain::NONE);
        const { assert!(Constrain::ANY.slide && Constrain::ANY.flip && Constrain::ANY.resize) };
    }
}
