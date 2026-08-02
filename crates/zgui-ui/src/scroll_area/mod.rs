//! A scrolling region with a scrollbar of its own.

mod bar;
mod style;

pub use crate::scroll_area::bar::{ScrollBar, ScrollBarProps};
pub use crate::scroll_area::style::ScrollAreaStyle;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};
use zgui_ui_primitives::Orientation;

/// What the scroll area's rules are installed under.
pub(crate) const SHEET: &str = "zui-scroll-area";

/// What a scrollbar reads to know how far along its container is.
#[derive(Copy, Clone)]
pub struct ScrollAreaContext {
    /// The element that actually scrolls.
    viewport: NodeRef,
    /// Where it is, observed rather than read once.
    position: Signal<ScrollPosition, LocalStorage>,
}

impl ScrollAreaContext {
    /// The scroll area the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// The element that scrolls.
    #[must_use]
    pub fn viewport(self) -> NodeRef {
        self.viewport
    }

    /// Where the container is now: its offset, its content extent and its visible extent.
    ///
    /// This is the observation channel's own signal, so a scrollbar built on it is written during
    /// the frame the scroll happens and is painted in its new place in that same frame.
    #[must_use]
    pub fn position(self) -> ScrollPosition {
        self.position.get()
    }

    /// Scrolls the container to an absolute offset along one axis, leaving the other alone.
    pub fn scroll_to(self, orientation: Orientation, offset: f32) {
        let now = self.position.get_untracked().offset;
        let next = match orientation {
            Orientation::Vertical => zgui::geom::Point::new(now.x, zgui::geom::DevicePx(offset)),
            _ => zgui::geom::Point::new(zgui::geom::DevicePx(offset), now.y),
        };
        self.viewport
            .scroll_to(ScrollTarget::Offset(next), ScrollBehavior::Instant);
    }
}

/// A region that scrolls, with a scrollbar the keyboard can reach.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A long list in a short box.
/// #[component]
/// fn Names() -> impl IntoView {
///     view! {
///         ScrollArea(class = "h-64") {
///             column {
///                 text {"Ada"}
///                 text {"Grace"}
///                 text {"Barbara"}
///             }
///         }
///     }
/// }
/// ```
///
/// # Why the bar is not drawn here
///
/// Because the engine already draws one. Every scrolling box reserves a gutter for a scrollbar, and
/// a track and a thumb are composed into it from the same content extent the layout produced. A
/// second thumb drawn over that would be a second answer to one question, sampled at a different
/// moment from the first, and the frame in which the two disagree is the frame somebody is looking
/// at.
///
/// What this component adds is the half the engine has no way to express: a name, a role, a place
/// in the focus order and the keys that operate it. [`ScrollBar`] states what that leaves it doing.
///
/// # What a scroll costs
///
/// One bit. Scrolling marks the container and nothing else; the fragment pass composes the new
/// positions on the way past, and the thumb is one of the fragments it composes. Nothing is
/// restyled and nothing is laid out again.
///
/// # Keyboard
///
/// The scrollbars are focusable and operable: the arrows scroll by a line, <kbd>Page Up</kbd> and
/// <kbd>Page Down</kbd> by a screen, <kbd>Home</kbd> and <kbd>End</kbd> to the ends. The content
/// inside is reached by <kbd>Tab</kbd> exactly as it would be without a scroll area.
#[component]
pub fn ScrollArea(
    /// Which scrollbars to draw.
    #[prop(default = Orientation::Vertical)]
    orientation: Orientation,
    /// What the region is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Where to record this component's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the region's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What scrolls.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, ScrollAreaStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let viewport = NodeRef::new();
    let context = ScrollAreaContext {
        viewport,
        position: viewport.observe_scroll(),
    };
    provide_local_context(context);

    let mut semantics = A11yBinding::new(Role::ScrollView);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-scroll-area"), true)
        .attribute(
            zgui::view::AttrName::new("data-orientation"),
            orientation.name(),
        )
        .a11y_from(semantics);

    // Which bars exist is decided once, from a prop, and never again: a bar that came and went
    // would take its own track measurement with it, and a bar with no track cannot work out
    // whether it is needed.
    let down = matches!(orientation, Orientation::Vertical | Orientation::Both)
        .then(|| AnyView::new(view! { ScrollBar(orientation = Orientation::Vertical) }));
    let across = matches!(orientation, Orientation::Horizontal | Orientation::Both)
        .then(|| AnyView::new(view! { ScrollBar(orientation = Orientation::Horizontal) }));

    view! {
        box(node_ref = element, class = ScrollAreaStyle::CLASS, {..own}, {..attrs}, class = class) {
            scroll(class = "zui-scroll-area__viewport", node_ref = viewport) {
                {children.into_view_once()}
            }
            {down}
            {across}
        }
    }
}
