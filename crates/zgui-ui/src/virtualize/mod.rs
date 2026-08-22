//! Building only the rows somebody can see.
//!
//! Everything here is one idea: a list's length is data, and the number of *elements* a list costs
//! should be a function of the space it is shown in rather than of how much data there is. A
//! hundred thousand rows and a hundred rows cost the same, because the same thirty elements are
//! mounted either way and the rest of the extent is two empty boxes.
//!
//! [`Virtualize`] is the mechanism and takes no view of its own; [`VirtualList`] is the ordinary
//! list built on it, and [`DataTable`](crate::data_table::DataTable) uses the same mechanism for
//! its body.

mod list;
mod style;
mod window;

pub use crate::virtualize::list::{VirtualList, VirtualListProps};
pub use crate::virtualize::style::VirtualListStyle;
pub use crate::virtualize::window::{MAX_EXTENT, VirtualWindow, window};

use zgui::prelude::*;
use zgui::reactive::LocalStorage;

/// The rows of a long list that are worth building, followed live.
///
/// Construct one with the scroll container it watches, and read [`Virtualize::window`] from inside
/// a keyed list. It subscribes to the container's scroll position through the observation channel,
/// so it is recomputed during the frame a scroll happens in and before anything is painted — and
/// only when the *window* changes does anything rebuild, so scrolling within one row costs nothing
/// at all.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::virtualize::Virtualize;
///
/// /// A hundred thousand rows, of which a couple of dozen exist.
/// #[component]
/// fn Ledger() -> impl IntoView {
///     let port = NodeRef::new();
///     let rows = RwSignal::new_local(100_000_usize);
///     let seen = Virtualize::new(port, rows.into(), 24.0, 4);
///
///     view! {
///         scroll(node_ref = port) {
///             column(
///                 style:padding-top = move || Some(format!("{}px", seen.window().lead)),
///                 style:padding-bottom = move || Some(format!("{}px", seen.window().trail))
///             ) {
///                 for index in move || seen.window().indices(), key = |index: &usize| *index {
///                     text {{move || format!("row {index}")}}
///                 }
///             }
///         }
///     }
/// }
/// ```
///
/// # Why the row height is a declaration and not a measurement
///
/// A window is decided *before* the rows in it are built, so it cannot be decided from their
/// heights: measuring row 4 200 means building row 4 200, which is the cost virtualisation exists
/// to avoid. Every virtualised list in this library is therefore a fixed-height list, the height is
/// declared, and a style sheet that disagrees with the declaration produces a scrollbar of the
/// wrong length rather than a wrong set of rows.
///
/// The declaration is a signal, so an application that lets a person choose the row height moves
/// the list that is open rather than the next one it builds. A plain number is a declaration that
/// never moves, and reads the same at the call site.
#[derive(Copy, Clone)]
pub struct Virtualize {
    /// The window, recomputed whenever the scroll position or the row count changes.
    window: Signal<VirtualWindow, LocalStorage>,
}

impl Virtualize {
    /// Watches `viewport`, a scroll container holding `rows` rows of `row_size` CSS pixels each.
    ///
    /// `overscan` is how many rows to build beyond each edge of the port.
    ///
    /// The handle observes the container's scroll position, which is a share in one observation
    /// held for as long as the calling scope lives. It is taken when `viewport` binds rather than
    /// now, so calling this from a component's body — before the element it names exists — is the
    /// ordinary way to use it.
    #[must_use]
    pub fn new(
        viewport: NodeRef,
        rows: Signal<usize, LocalStorage>,
        row_size: impl Into<Signal<f32, LocalStorage>>,
        overscan: usize,
    ) -> Self {
        let row_size = row_size.into();
        let scroll = viewport.observe_scroll();
        Self {
            window: Signal::derive_local(move || {
                let position = scroll.get();
                // The observation answers in device pixels and the row height was declared in CSS
                // pixels. Dividing here rather than multiplying the row height keeps the whole of
                // the rest of this module in one space.
                let scale = viewport.scale();
                let scale = if scale.is_finite() && scale > 0.0 {
                    scale
                } else {
                    1.0
                };
                window(
                    rows.get(),
                    row_size.get(),
                    position.scrollport.height.0 / scale,
                    position.offset.y.0 / scale,
                    overscan,
                )
            }),
        }
    }

    /// The rows worth building right now, subscribing to it.
    #[must_use]
    pub fn window(&self) -> VirtualWindow {
        self.window.get()
    }

    /// The same, without subscribing.
    #[must_use]
    pub fn window_untracked(&self) -> VirtualWindow {
        self.window.get_untracked()
    }

    /// The indices worth building right now, which is what a keyed list is driven by.
    #[must_use]
    pub fn indices(&self) -> Vec<usize> {
        self.window.get().indices()
    }
}

#[cfg(test)]
mod tests {
    use zgui::geom::{DevicePx, Point, Size};
    use zgui::prelude::*;
    use zgui::reactive::RwSignal;
    use zgui::view::{Dom, ObservedValue, ScrollPosition};
    use zgui_testkit_view::Window;

    use super::Virtualize;

    /// The scroll position a port of `port` device pixels scrolled to `offset` reports.
    fn scrolled(offset: f32, port: f32, content: f32) -> ScrollPosition {
        ScrollPosition {
            offset: Point::new(DevicePx(0.0), DevicePx(offset)),
            content_size: Size::new(DevicePx(200.0), DevicePx(content)),
            scrollport: Size::new(DevicePx(200.0), DevicePx(port)),
        }
    }

    #[test]
    fn the_window_follows_the_scroll_signal_rather_than_a_read() {
        let window = Window::open();
        let port = window
            .dom
            .create_element(zgui::view::ElementName::new("scroll"));
        window.dom.insert(window.root, port, None);

        let handle = window.scope.with(NodeRef::new);
        let rows = window.scope.with(|| RwSignal::new_local(10_000_usize));
        let seen = window
            .scope
            .with(|| Virtualize::new(handle, rows.into(), 20.0, 2));
        handle.bind(port, &window.dom_handle, &window.host_handle);
        window.frame();

        window.dom.deliver(
            port,
            ObservedValue::ScrollPosition(scrolled(0.0, 400.0, 200_000.0)),
        );
        window.frame();
        assert_eq!(seen.window_untracked().first, 0);
        assert_eq!(
            seen.window_untracked().count,
            23,
            "21 visible plus 2 of slack"
        );

        window.dom.deliver(
            port,
            ObservedValue::ScrollPosition(scrolled(2_000.0, 400.0, 200_000.0)),
        );
        window.frame();
        let moved = seen.window_untracked();
        assert_eq!(moved.first, 98, "a hundred rows down, less the overscan");
        assert_eq!(moved.count, 25);
        assert_eq!(moved.lead, 1_960.0);
    }

    #[test]
    fn a_surface_with_two_device_pixels_per_css_pixel_shows_the_same_rows() {
        let window = Window::open();
        window.host.set_scale(2.0);
        let port = window
            .dom
            .create_element(zgui::view::ElementName::new("scroll"));
        window.dom.insert(window.root, port, None);

        let handle = window.scope.with(NodeRef::new);
        let rows = window.scope.with(|| RwSignal::new_local(10_000_usize));
        let seen = window
            .scope
            .with(|| Virtualize::new(handle, rows.into(), 20.0, 2));
        handle.bind(port, &window.dom_handle, &window.host_handle);
        window.frame();

        // The same 400 CSS-pixel port and the same 2 000 CSS-pixel offset, in device pixels.
        window.dom.deliver(
            port,
            ObservedValue::ScrollPosition(scrolled(4_000.0, 800.0, 400_000.0)),
        );
        window.frame();
        assert_eq!(
            seen.window_untracked().first,
            98,
            "a retina surface must not show a different hundred rows",
        );
        assert_eq!(seen.window_untracked().count, 25);
    }
}
