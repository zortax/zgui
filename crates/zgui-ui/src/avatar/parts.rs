//! The pieces an [`Avatar`](crate::Avatar) is composed from when the caller composes it.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

use crate::avatar::style::AvatarStyle;
use crate::avatar::{AvatarSize, SHEET};

/// The picture inside an [`Avatar`](crate::Avatar).
///
/// It fills the whole circle and is cropped square, so a portrait and a landscape both come out as
/// the same disc with the middle of the frame in it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::avatar::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Avatar(label = "Ada Lovelace") {
///         AvatarFallback {"AL"}
///         AvatarImage(src = "/avatars/ada.png")
///     }
/// }
/// # }
/// ```
///
/// # Where it goes among the children
///
/// After the fallback. The two are stacked rather than swapped, so whichever is written last is
/// the one on top — and a picture that never loads leaves the letters showing without anything
/// having to notice that it did not.
#[component]
pub fn AvatarImage(
    /// Where the picture is.
    #[prop(into, default = Signal::stored_local(None))]
    src: Signal<Option<String>, LocalStorage>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, AvatarStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::Image).hidden(true));

    view! {
        image(
            class = "zui-avatar__image",
            src = move || src.get(),
            {..own},
            {..attrs},
            class = class
        )
    }
}

/// What an [`Avatar`](crate::Avatar) shows when there is no picture: initials, usually.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::avatar::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { Avatar(label = "Ada Lovelace") {AvatarFallback {"AL"}} }
/// # }
/// ```
#[component]
pub fn AvatarFallback(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The letters, or whatever stands in for the face.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, AvatarStyle::CSS);
    view! {
        box(class = "zui-avatar__fallback", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// A mark pinned to the bottom corner of an [`Avatar`](crate::Avatar): presence, or a count.
///
/// It sizes itself from the avatar it is in — eight pixels on a small one, ten on an ordinary one,
/// twelve on a large one — and is ringed in the page colour so that it reads as sitting on the
/// picture rather than in it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::avatar::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Avatar(label = "Ada Lovelace") {
///         AvatarFallback {"AL"}
///         AvatarBadge(label = "Online")
///     }
/// }
/// # }
/// ```
#[component]
pub fn AvatarBadge(
    /// What the mark means, for a reader. Without one it is decoration and is not announced.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What is drawn in it, when it is not just a dot.
    #[prop(optional)]
    children: Option<Children>,
) -> impl IntoView {
    install_stylesheet(SHEET, AvatarStyle::CSS);
    let own = Attrs::new().a11y_from(match label {
        Some(text) => A11yBinding::new(Role::Image).label(text),
        None => A11yBinding::new(Role::GenericContainer).hidden(true),
    });

    view! {
        box(class = "zui-avatar__badge", {..own}, {..attrs}, class = class) {
            {children.map(Children::into_view_once)}
        }
    }
}

/// Several [`Avatar`](crate::Avatar)s overlapping in a row.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::avatar::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     AvatarGroup {
///         Avatar(label = "Ada") {AvatarFallback {"AL"}}
///         Avatar(label = "Grace") {AvatarFallback {"GH"}}
///         AvatarGroupCount {"+3"}
///     }
/// }
/// # }
/// ```
///
/// Each member is ringed in the page colour, which is what turns a pile of overlapping discs into
/// a legible stack: the ring is the gap between one face and the next.
#[component]
pub fn AvatarGroup(
    /// How large the faces in it are. The count at the end takes this too.
    #[prop(default = AvatarSize::Md)]
    size: AvatarSize,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The avatars, and whatever counts the rest of them.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, AvatarStyle::CSS);
    // The size is stated on the group rather than read back off the faces inside it. A rule that
    // sized the count from its neighbours would be a relative selector, which this engine does not
    // have — see the parity register's `:has()` row — so the group is told once and the count reads
    // it from the element above it.
    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-size"), move || {
            Some(size.name().to_string())
        })
        .a11y_from(A11yBinding::new(Role::Group));

    view! {
        box(class = "zui-avatar-group", {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// How many more people there are than an [`AvatarGroup`] has room to show.
///
/// It takes the size the [`AvatarGroup`] was given, so a group of large faces ends in a large disc.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # use zgui_ui::avatar::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { AvatarGroup {Avatar {AvatarFallback {"AL"}} AvatarGroupCount {"+3"}} }
/// # }
/// ```
#[component]
pub fn AvatarGroupCount(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The count.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, AvatarStyle::CSS);
    view! {
        box(class = "zui-avatar-group__count", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
