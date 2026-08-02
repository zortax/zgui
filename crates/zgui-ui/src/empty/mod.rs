//! What a region says when there is nothing in it yet.

mod parts;
mod style;

pub use crate::empty::parts::{
    EmptyContent, EmptyContentProps, EmptyDescription, EmptyDescriptionProps, EmptyHeader,
    EmptyHeaderProps, EmptyMedia, EmptyMediaProps, EmptyMediaVariant, EmptyMediaVariants,
    EmptyTitle, EmptyTitleProps,
};
pub use crate::empty::style::EmptyStyle;

use zgui::prelude::*;
use zgui::{component, view};

/// What the empty state's rules are installed under.
pub(crate) const SHEET: &str = "zui-empty";

/// A centred panel standing where a list, a table or a folder would be.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// An inbox with nothing in it.
/// #[component]
/// fn NoMessages() -> impl IntoView {
///     view! {
///         Empty {
///             EmptyHeader {
///                 EmptyMedia(variant = EmptyMediaVariant::Icon) {"✉"}
///                 EmptyTitle {"No messages"}
///                 EmptyDescription {"Anything sent to you will show up here."}
///             }
///             EmptyContent {Button {"Write one"}}
///         }
///     }
/// }
/// ```
///
/// # Why it has a way out
///
/// An empty state that only says *empty* leaves a user where they started. The panel is built
/// around [`EmptyContent`], which is where the thing to do next goes — and a state with nothing in
/// that slot is worth a second look, because almost every emptiness has an action that ends it.
///
/// # What a reader is told
///
/// Whatever the title and description say. The panel is a layout rather than a control, so it
/// carries no role and adds no announcement of its own.
#[component]
pub fn Empty(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The heading, and what to do about it.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, EmptyStyle::CSS);
    view! {
        box(class = EmptyStyle::CLASS, {..attrs}, class = class) {{children.into_view_once()}}
    }
}
