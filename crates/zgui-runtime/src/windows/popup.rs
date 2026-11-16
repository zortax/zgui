//! What a pop-up window is asked for before it exists.

use zgui_geom::{Css, CssPx, Rect, Size};
use zgui_platform::{
    Anchor, Constrain, InputSerial, PopupPlacement, SurfaceAttributes, SurfaceId, SurfaceRole,
};

/// Where a pop-up sits against its parent window, and whether it takes the pointer and keyboard.
///
/// A pop-up is a menu, a drop-down or a tooltip that extends past the edges of the window that
/// owns it. The parent is an ordinary window, another pop-up or a shell window. The desktop
/// places the pop-up: it hangs from a point of a rectangle on the parent and extends in one
/// direction, and the desktop moves it as [`Constrain`] allows to keep it on the screen.
///
/// A menu takes a grab with the serial of the press that opens it. Read the serial in the press
/// handler with [`WindowHandle::input_serial`](crate::windows::WindowHandle::input_serial). The
/// desktop then closes the menu with
/// [`CloseReason::Dismissed`](zgui_platform::CloseReason::Dismissed) when the user presses outside
/// it.
///
/// ```
/// use zgui_geom::{CssPx, Point, Rect, Size};
/// use zgui_platform::{Anchor, Constrain, InputSerial};
/// use zgui_runtime::windows::PopupOptions;
///
/// // A menu below a bar item 34 pixels high, centred on it, flipped up when there is no room.
/// let item = Rect::new(
///     Point::new(CssPx(790.0), CssPx(0.0)),
///     Size::new(CssPx(20.0), CssPx(34.0)),
/// );
/// let menu = PopupOptions::new(item)
///     .with_anchor(Anchor::Bottom)
///     .with_gravity(Anchor::Bottom)
///     .with_constrain(Constrain::ANY)
///     .with_grab(Some(InputSerial::new(7)))
///     .with_size(200.0, 150.0);
/// assert_eq!(menu.placement().grab, Some(InputSerial::new(7)));
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct PopupOptions {
    /// Where the pop-up hangs from. The parent is filled in when the pop-up opens.
    placement: PopupPlacement,
    /// The size the pop-up starts at, in CSS pixels.
    size: Option<Size<CssPx, Css>>,
}

impl PopupOptions {
    /// A pop-up under `anchor_rect`, aligned to its left, sliding to stay on the screen.
    ///
    /// The rectangle is in the parent's own coordinates, in CSS pixels. The pop-up takes no grab
    /// until [`PopupOptions::with_grab`] gives it one.
    pub fn new(anchor_rect: Rect<CssPx, Css>) -> Self {
        Self {
            placement: PopupPlacement::below(SurfaceId::new(0), anchor_rect),
            size: None,
        }
    }

    /// Sets the point of the anchor rectangle the pop-up hangs from.
    pub fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.placement.anchor = anchor;
        self
    }

    /// Sets the direction the pop-up extends in from the anchor point.
    pub fn with_gravity(mut self, gravity: Anchor) -> Self {
        self.placement.gravity = gravity;
        self
    }

    /// Sets what the desktop may do to a pop-up that would not fit on the screen.
    pub fn with_constrain(mut self, constrain: Constrain) -> Self {
        self.placement.constrain = constrain;
        self
    }

    /// Takes the pointer and the keyboard against the press `serial`, or takes no grab.
    ///
    /// A desktop that cannot read the serial dismisses the pop-up at once, so pass the serial of
    /// the press that opens it.
    pub fn with_grab(mut self, serial: Option<InputSerial>) -> Self {
        self.placement.grab = serial;
        self
    }

    /// Sets the size the pop-up starts at, in CSS pixels.
    pub fn with_size(mut self, width: f32, height: f32) -> Self {
        self.size = Some(Size::new(CssPx(width), CssPx(height)));
        self
    }

    /// Where the pop-up hangs from and what the desktop may do to it.
    pub fn placement(&self) -> &PopupPlacement {
        &self.placement
    }

    /// The size the pop-up starts at, if one was set.
    pub fn size(&self) -> Option<Size<CssPx, Css>> {
        self.size
    }

    /// Makes `attributes` describe this pop-up, keeping its title, its identifier and its
    /// transparency.
    pub(crate) fn apply_to(self, attributes: &mut SurfaceAttributes) {
        let popup = SurfaceAttributes::popup(self.placement);
        attributes.role = popup.role;
        attributes.resizable = popup.resizable;
        attributes.decorations = popup.decorations;
        if self.size.is_some() {
            attributes.size = self.size;
        }
    }
}

/// Points the pop-up `attributes` describe at the surface `parent`.
pub(crate) fn set_parent(attributes: &mut SurfaceAttributes, parent: SurfaceId) {
    if let SurfaceRole::Popup(placement) = &mut attributes.role {
        placement.parent = parent;
    }
}

#[cfg(test)]
mod tests {
    use super::{PopupOptions, set_parent};
    use zgui_geom::{CssPx, Point, Rect, Size};
    use zgui_platform::{
        Anchor, Constrain, Decorations, InputSerial, SurfaceAttributes, SurfaceId, SurfaceRole,
    };

    fn item() -> Rect<CssPx, zgui_geom::Css> {
        Rect::new(
            Point::new(CssPx(10.0), CssPx(0.0)),
            Size::new(CssPx(20.0), CssPx(34.0)),
        )
    }

    #[test]
    fn every_option_reaches_the_placement() {
        let options = PopupOptions::new(item())
            .with_anchor(Anchor::Bottom)
            .with_gravity(Anchor::Bottom)
            .with_constrain(Constrain::ANY)
            .with_grab(Some(InputSerial::new(3)))
            .with_size(200.0, 150.0);
        let placement = options.placement();
        assert_eq!(placement.anchor_rect, item());
        assert_eq!(placement.anchor, Anchor::Bottom);
        assert_eq!(placement.gravity, Anchor::Bottom);
        assert_eq!(placement.constrain, Constrain::ANY);
        assert_eq!(placement.grab, Some(InputSerial::new(3)));
        assert_eq!(options.size(), Some(Size::new(CssPx(200.0), CssPx(150.0))));
    }

    #[test]
    fn a_pop_up_takes_no_grab_unless_it_is_given_a_press() {
        assert_eq!(PopupOptions::new(item()).placement().grab, None);
    }

    #[test]
    fn a_pop_up_is_undecorated_and_its_parent_is_filled_in_when_it_opens() {
        let mut attributes = SurfaceAttributes::new("menu");
        attributes.transparent = true;
        PopupOptions::new(item()).apply_to(&mut attributes);
        assert_eq!(attributes.decorations, Decorations::None);
        assert!(!attributes.resizable);
        assert!(attributes.transparent);
        assert_eq!(attributes.title.as_str(), "menu");
        set_parent(&mut attributes, SurfaceId::new(4));
        let SurfaceRole::Popup(placement) = &attributes.role else {
            panic!("a pop-up keeps its role");
        };
        assert_eq!(placement.parent, SurfaceId::new(4));
    }
}
