//! The keyboard's handle on a scroll area.

use zgui::prelude::*;
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};
use zgui_ui_primitives::Orientation;

use crate::scroll_area::style::ScrollAreaStyle;
use crate::scroll_area::{SHEET, ScrollAreaContext};

/// How far one arrow key scrolls, in CSS pixels.
const LINE: f32 = 40.0;

/// One scrollbar of a [`ScrollArea`](crate::ScrollArea).
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui_primitives::Orientation;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { ScrollArea(orientation = Orientation::Both) {text {"a long document"}} }
/// # }
/// ```
///
/// # What this draws, and what it does not
///
/// It draws no thumb. The engine composes a track and a thumb into the gutter every scrolling box
/// reserves, and a region with two of them is worse than a region with either: they are two answers
/// to one question, sampled at different moments, and the frame where they disagree is the frame
/// somebody is looking at. So the bar on the screen is the engine's, and what this contributes is
/// the half the engine has no way to express — a name, a role, a place in the focus order and the
/// keys that operate it, over exactly the strip the engine drew into.
///
/// The element takes no pointer events, which is what lets a press reach the engine's own bar
/// underneath it: dragging the thumb and paging the track are the engine's, and doing them here as
/// well would be two implementations of one gesture racing each other for the same offset.
///
/// It is still always built, and still carries `data-scrollable` — a control that vanished when
/// there was nothing to scroll would take its own tab stop and its own announcement with it, and
/// would have to come back the moment the content grew.
///
/// # Keyboard
///
/// The arrows scroll by a line, <kbd>Page Up</kbd> and <kbd>Page Down</kbd> by a screen, and
/// <kbd>Home</kbd> and <kbd>End</kbd> go to the ends.
///
/// # What a reader is told
///
/// That it is a scrollbar, which container it controls, which way it runs, and where along its
/// range it is — so that it can be operated by someone who is not looking at it, which is the only
/// reason a scrollbar of one's own is allowed to exist at all.
#[component]
pub fn ScrollBar(
    /// Which axis this bar scrolls.
    #[prop(default = Orientation::Vertical)]
    orientation: Orientation,
    /// Classes merged after the bar's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, ScrollAreaStyle::CSS);
    let context = ScrollAreaContext::current();
    let element = NodeRef::new();
    let vertical = matches!(orientation, Orientation::Vertical);

    // Everything below is in CSS pixels, the unit a style sheet is written in. The observation
    // reports device pixels, and on any display that is not exactly 1× the two differ.
    let scale = move || {
        let scale = element.scale();
        if scale > 0.0 { scale } else { 1.0 }
    };
    let extents = move || {
        let position = context?.position();
        let (content, viewport, offset) = if vertical {
            (
                position.content_size.height.0,
                position.scrollport.height.0,
                position.offset.y.0,
            )
        } else {
            (
                position.content_size.width.0,
                position.scrollport.width.0,
                position.offset.x.0,
            )
        };
        Some((content / scale(), viewport / scale(), offset / scale()))
    };
    /// Whether there is enough of a difference between two extents to scroll.
    fn scrollable(extents: Option<(f32, f32, f32)>) -> bool {
        extents.is_some_and(|(content, viewport, _)| content - viewport > 0.5)
    }

    let scroll_by = move |by_css: f32| {
        let (Some(context), Some((content, viewport, offset))) = (context, extents()) else {
            return;
        };
        let to = (offset + by_css).clamp(0.0, (content - viewport).max(0.0));
        context.scroll_to(orientation, to * scale());
    };

    let semantics = A11yBinding::new(Role::ScrollBar)
        .orientation(if vertical {
            zgui::vocab::Orientation::Vertical
        } else {
            zgui::vocab::Orientation::Horizontal
        })
        .numeric_value(move || f64::from(extents().map_or(0.0, |(_, _, offset)| offset)))
        .step(move |a11y| {
            let (content, viewport, _) = extents().unwrap_or((0.0, 0.0, 0.0));
            a11y.numeric_range(0.0, f64::from((content - viewport).max(0.0)))
                .numeric_step(f64::from(LINE))
        });
    let semantics = match context {
        Some(context) => semantics.controls(context.viewport()),
        None => semantics,
    };

    let own = Attrs::new()
        .attribute(
            zgui::view::AttrName::new("data-orientation"),
            orientation.name(),
        )
        .attribute(zgui::view::AttrName::new("data-scrollable"), move || {
            Some(
                if scrollable(extents()) {
                    "true"
                } else {
                    "false"
                }
                .to_owned(),
            )
        })
        .a11y_from(semantics);

    view! {
        control(
            class = "zui-scroll-area__bar",
            node_ref = element,
            // Only while there is something to scroll: a fixed tab stop would leave a keyboard user
            // landing on a bar nothing moves.
            tabindex = move || {
                if scrollable(extents()) { Focus::Sequential } else { Focus::Programmatic }
            },
            on:key_down = move |ev| {
                let Some((content, viewport, _)) = extents() else { return };
                let page = viewport.max(LINE);
                let by = match &ev.key {
                    Key::Named(NamedKey::ArrowDown | NamedKey::ArrowRight) => LINE,
                    Key::Named(NamedKey::ArrowUp | NamedKey::ArrowLeft) => -LINE,
                    Key::Named(NamedKey::PageDown) => page,
                    Key::Named(NamedKey::PageUp) => -page,
                    Key::Named(NamedKey::Home) => -content,
                    Key::Named(NamedKey::End) => content,
                    _ => return,
                };
                scroll_by(by);
                // Claimed, so the same key does not also scroll whatever is outside the area.
                ev.prevent_default();
                ev.stop_propagation();
            },
            {..own},
            {..attrs},
            class = class
        )
    }
}
