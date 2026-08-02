//! Typing a letter in a menu, and what it lands on.

use zgui::prelude::*;
use zgui::vocab::Key;
use zgui::{component, view};
use zgui_ui_primitives::RovingContext;

use crate::menu::SHEET;
use crate::menu::style::MenuStyle;
use crate::menu::typeahead::{Typeahead, matching};

/// Moves the keyboard to the next item that reads as beginning with what was typed.
///
/// It renders an element of no appearance inside the roving-focus group, and that position is the
/// design rather than an accident: the group's items are below it, so their key presses bubble
/// through here, and the group itself is above it, so its own arrow-key handling still runs. A
/// listener anywhere else would either not see the presses or see them before the group did.
///
/// ```no_run
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::menu::{MenuTypeahead, MenuTypeaheadProps};
/// use zgui_ui_primitives::prelude::*;
///
/// /// A list whose items can be reached by typing their first letters.
/// #[component]
/// fn Searchable(children: Children) -> impl IntoView {
///     view! {
///         RovingFocus(orientation = Orientation::Vertical) {
///             MenuTypeahead {{children.into_view_once()}}
///         }
///     }
/// }
/// ```
#[component]
pub fn MenuTypeahead(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The items.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, MenuStyle::CSS);
    let group = RovingContext::current();
    let typed = Typeahead::new();

    let on_key_down = handler(
        events::KEY_DOWN,
        move |ev: &mut EventCx<'_, events::KeyDown>| {
            let Key::Character(character) = &ev.key else {
                // Not a character at all. The arrow keys and the ends belong to the group around
                // this one, and swallowing them here would stop the menu being walked.
                return;
            };
            // A space is what activates the item the keyboard is on, so it is only ever a
            // character to search for once a search is already under way.
            if character.as_ref() == " " && typed.buffer().is_empty() {
                return;
            }
            let Some(group) = group else {
                return;
            };
            let search = {
                typed.push(character.as_ref());
                typed.search()
            };
            let items = group.collection().items_untracked();
            let Some(found) = matching(&items, group.active(), &search) else {
                return;
            };
            group.set_active(found.id());
            found.focus();
            ev.prevent_default();
            ev.stop_propagation();
        },
    );

    view! {
        box(class = "zui-menu__keys", on:key_down = on_key_down, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
