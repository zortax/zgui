//! The scope in which options say what they are without being on the screen.

use zgui::prelude::*;
use zgui::view::AttrName;
use zgui::{component, view};

/// Marks a scope in which an option describes itself rather than joining the list.
///
/// The one thing it changes: an option built below this teaches its enclosing
/// [`Listbox`](crate::listbox::Listbox) what its value reads as and registers nothing. Without that
/// distinction, mounting the options twice would give a list twice as long as it looks, with two
/// entries per value for the arrow keys to walk and for <kbd>Enter</kbd> to choose between.
#[derive(Copy, Clone)]
pub struct ListboxCatalogue;

impl ListboxCatalogue {
    /// Whether the calling scope is one where options describe themselves.
    #[must_use]
    pub fn is_current() -> bool {
        use_local_context::<Self>().is_some()
    }
}

/// Builds its children only so that they say what they are.
///
/// Nothing here is drawn, laid out, read aloud or pressed: the element carries `hidden`, which
/// generates no box, and a subtree that generates no box is out of the accessibility tree with it.
/// What it is for is the one thing that does survive — an option's text, which is the text it
/// renders and so cannot be known until it has been built.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::listbox::{ListboxCatalogueOf, ListboxCatalogueOfProps};
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { ListboxCatalogueOf {text {"Pound sterling"}} }
/// # }
/// ```
///
/// A caller mounts this exactly while the real list is not mounted, so no value is described twice
/// over — and the two overlapping for the length of a closing animation costs nothing, because what
/// one writes the other writes identically.
#[component]
pub fn ListboxCatalogueOf(
    /// The options, built for what they say rather than for what they show.
    children: ChildrenFn,
) -> impl IntoView {
    provide_local_context(ListboxCatalogue);
    let own = Attrs::new()
        .attribute(AttrName::new("hidden"), || Some("true".to_owned()))
        .a11y_from(A11yBinding::unspecified().hidden(|| true));

    view! { box(class = "zui-listbox__catalogue", {..own}) {{children.view()}} }
}
