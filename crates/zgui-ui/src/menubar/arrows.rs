//! Moving along the bar from inside a menu that is already open.

use zgui::prelude::*;
use zgui::vocab::{Key, NamedKey};
use zgui::{component, view};
use zgui_ui_primitives::RovingContext;

use crate::menubar::SHEET;
use crate::menubar::style::MenubarStyle;

/// Carries <kbd>←</kbd> and <kbd>→</kbd> from an open menu back out to the bar it belongs to.
///
/// Written for [`MenubarContent`](crate::MenubarContent), which uses it around its own items; a
/// caller laying out a menu surface itself wraps the items in one of these and hands it the bar's
/// group, read from a scope that is still inside the bar.
///
/// Every desktop menubar does this and it is the reason a bar is worth having over a row of
/// unrelated dropdowns: with *File* open, one press of <kbd>→</kbd> is *Edit* open. Without it the
/// only way from one menu to the next is <kbd>Escape</kbd>, an arrow and <kbd>↓</kbd>.
///
/// It has to be written here rather than left to the bar's own arrow keys, because the surface a
/// menu opens is portalled onto an overlay band: it is nowhere near the bar in the tree, so a key
/// pressed on one of its items bubbles past the overlay root and never reaches the bar at all. The
/// bar's group is therefore read where the menu is *written* — which is inside the bar — and handed
/// to this, which is inside the surface.
///
/// Stepping the bar's group moves the focus onto the next name, and a name that takes the focus
/// while some menu is open opens its own. So this presses no menu open itself: it moves the focus
/// and the bar does the rest, exactly as it does when a pointer walks the same route.
#[component]
pub fn MenubarArrows(
    /// The bar's own roving-focus group, read outside the menu's.
    ///
    /// `None` for a menu written outside a bar, which leaves the arrows alone: there is nothing to
    /// move along, and swallowing them would stop whatever is around it from scrolling.
    bar: Option<RovingContext>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The items.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenubarStyle::CSS);

    let on_key_down = handler(
        events::KEY_DOWN,
        move |ev: &mut EventCx<'_, events::KeyDown>| {
            let steps = match ev.key {
                Key::Named(NamedKey::ArrowRight) => 1,
                Key::Named(NamedKey::ArrowLeft) => -1,
                _ => return,
            };
            let Some(bar) = bar else { return };
            if !bar.step(steps) {
                return;
            }
            // Only when something moved. A bar of one menu answers neither arrow, and a key
            // swallowed there is a key the surface around it never sees.
            ev.prevent_default();
            ev.stop_propagation();
        },
    );

    view! {
        box(class = "zui-menubar__arrows", on:key_down = on_key_down, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
