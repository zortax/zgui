//! A person's picture, and what to show when there is not one.

mod parts;
mod style;

pub use crate::avatar::parts::{
    AvatarBadge, AvatarBadgeProps, AvatarFallback, AvatarFallbackProps, AvatarGroup,
    AvatarGroupCount, AvatarGroupCountProps, AvatarGroupProps, AvatarImage, AvatarImageProps,
};
pub use crate::avatar::style::AvatarStyle;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, variants, view};

use crate::support::variant_attrs;

/// What the avatar's rules are installed under.
pub(crate) const SHEET: &str = "zui-avatar";

variants! {
    /// The axes an [`Avatar`] varies along.
    pub AvatarVariants {
        base: "zui-avatar",
        size: { Sm => "zui-avatar--sm", Md => "", Lg => "zui-avatar--lg" } = Md,
    }
}

/// A picture standing for a person, with something behind it for when there is no picture.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
/// use zgui_ui::avatar::*;
///
/// /// Whoever is signed in.
/// #[component]
/// fn Me() -> impl IntoView {
///     view! { Avatar(src = "/avatars/ada.png", label = "Ada Lovelace") {"AL"} }
/// }
/// ```
///
/// # The fallback, and why it is always there
///
/// The children are drawn behind the picture rather than instead of it. A picture that has not
/// arrived yet, and one whose address was wrong, are two states a component cannot tell apart at
/// the moment it mounts — so both show the initials, and a picture that does arrive covers them.
/// Nothing flickers, because nothing is swapped.
///
/// # Composing one instead
///
/// `src` is the short way to write the common case. The long way is [`AvatarImage`] and
/// [`AvatarFallback`] as children, which is what to reach for when either piece needs props of its
/// own — and it is the only way to add an [`AvatarBadge`], because a badge sits *on* the picture
/// rather than under it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::avatar::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Avatar(size = AvatarSize::Lg, label = "Ada Lovelace") {
///         AvatarFallback {"AL"}
///         AvatarImage(src = "/avatars/ada.png")
///         AvatarBadge(label = "Online")
///     }
/// }
/// # }
/// ```
///
/// # What a reader is told
///
/// One name for the whole thing, from `label`, and nothing from the picture or the initials
/// separately: an avatar is one item, and a reader that met a picture, a name and a pair of
/// letters would report three.
#[component]
pub fn Avatar(
    /// Where the picture is.
    #[prop(into, default = Signal::stored_local(None))]
    src: Signal<Option<String>, LocalStorage>,
    /// How big it is.
    #[prop(default = AvatarSize::Md)]
    size: AvatarSize,
    /// Whose picture it is.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after the avatar's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What to show when there is no picture: initials, usually.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, AvatarStyle::CSS);
    let variants = AvatarVariants { size };
    let semantics = match label {
        Some(text) => A11yBinding::new(Role::Image).label(text),
        None => A11yBinding::new(Role::Image).hidden(true),
    };
    let own = variant_attrs(variants.classes(), variants.data_attributes()).a11y_from(semantics);

    view! {
        box(class = AvatarStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
            if move || src.get().is_some() {
                image(
                    class = "zui-avatar__image",
                    src = move || src.get(),
                    a11y:hidden = true
                )
            }
        }
    }
}
