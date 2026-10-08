//! Opening a context menu at a point the code names.

use zgui::geom::{Css, CssPx, Point};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};

use crate::overlay::OverlayState;

/// The place a [`ContextMenuTrigger`](crate::ContextMenuTrigger) opens its menu at.
///
/// The trigger provides one to everything inside it. A pointer request opens the menu at the
/// pointer through it. A region that opens its menu from the keyboard calls
/// [`open_at`](Self::open_at) with the point of the item the keyboard is on, so the menu stands
/// beside that item.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::geom::{CssPx, Point};
/// # use zgui_ui::context_menu::ContextMenuPlace;
/// fn open_from_the_keyboard(row_left: f32, row_bottom: f32) {
///     if let Some(place) = ContextMenuPlace::current() {
///         place.open_at(Point::new(CssPx(row_left), CssPx(row_bottom)));
///     }
/// }
/// ```
#[derive(Copy, Clone)]
pub struct ContextMenuPlace {
    /// The region the menu is asked for over.
    pub(crate) area: NodeRef,
    /// Where in the region the menu opens, in CSS pixels from its left edge.
    pub(crate) left: RwSignal<f32, LocalStorage>,
    /// Where in the region the menu opens, in CSS pixels from its top edge.
    pub(crate) top: RwSignal<f32, LocalStorage>,
    /// The menu.
    pub(crate) state: OverlayState,
}

impl ContextMenuPlace {
    /// The place of the trigger around the caller, if there is one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Opens the menu at `position`, in window CSS pixels.
    pub fn open_at(&self, position: Point<CssPx, Css>) {
        // The region's box is in device pixels and a position is in CSS pixels, so the box is
        // converted. The window's box of the region is the one a pointer position is measured
        // against.
        let scale = self.area.scale();
        let origin = self
            .area
            .window_bounds()
            .map_or((0.0, 0.0), |box_| (box_.origin.x.0, box_.origin.y.0));
        self.left.set(position.x.0 - origin.0 / scale);
        self.top.set(position.y.0 - origin.1 / scale);
        self.state.open();
    }
}
