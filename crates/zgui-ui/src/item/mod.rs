//! One row of a list: a mark, some words, and the controls that go with them.

mod parts;
mod style;

pub use crate::item::parts::{
    ItemActions, ItemActionsProps, ItemContent, ItemContentProps, ItemDescription,
    ItemDescriptionProps, ItemFooter, ItemFooterProps, ItemHeader, ItemHeaderProps, ItemMedia,
    ItemMediaProps, ItemMediaVariant, ItemMediaVariants, ItemTitle, ItemTitleProps,
};
pub use crate::item::style::{ItemGroupStyle, ItemPartStyle, ItemStyle};

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::{component, variants, view};

use crate::support::variant_attrs;

/// What an item's own rules are installed under.
pub(crate) const SHEET: &str = "zui-item";

/// What the rules for the pieces inside an item are installed under.
pub(crate) const PARTS_SHEET: &str = "zui-item-parts";

/// What a group's rules are installed under.
pub(crate) const GROUP_SHEET: &str = "zui-item-group";

/// What an [`Item`] tells the pieces inside it, and what they tell it back.
///
/// One thing at present: whether a description is mounted. A row with a description is two lines
/// tall and its mark belongs at the top rather than in the middle, and only the description knows
/// there is one — a rule asking "does this row hold a description" reaches down from a parent to a
/// descendant, which this engine does not match, so the row hands a signal down and the
/// description sets it. `Copy`, so a piece stores one without cloning.
#[derive(Copy, Clone)]
pub struct ItemContext {
    /// Whether a description is mounted inside this row.
    description: RwSignal<bool, LocalStorage>,
}

impl ItemContext {
    /// The row the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Whether a description is mounted in it.
    #[must_use]
    pub fn has_description(self) -> bool {
        self.description.get()
    }

    /// Says a description is mounted, and unsays it when the calling scope goes away.
    pub fn claim_description(self) {
        self.description.set(true);
        on_cleanup_local(move || {
            self.description.try_set(false);
        });
    }
}

variants! {
    /// The axes an [`Item`] varies along.
    pub ItemVariants {
        base: "zui-item",
        variant: {
            Default => "",
            Outline => "zui-item--outline",
            Muted => "zui-item--muted",
        } = Default,
        size: { Md => "", Sm => "zui-item--sm" } = Md,
    }
}

/// A row of content with a mark on one side and controls on the other.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A file, and the one thing to do with it.
/// #[component]
/// fn Attachment() -> impl IntoView {
///     view! {
///         Item(variant = ItemVariant::Outline) {
///             ItemMedia(variant = ItemMediaVariant::Icon) {"📄"}
///             ItemContent {
///                 ItemTitle {"report.pdf"}
///                 ItemDescription {"1.2 MB"}
///             }
///             ItemActions {Button(variant = ButtonVariant::Ghost) {"Remove"}}
///         }
///     }
/// }
/// ```
///
/// # An item and a menu item
///
/// This one is *content*: it is read, and the controls inside it are what can be operated. A
/// [`MenuItem`](crate::MenuItem) is the other thing — the row itself is the control, and pressing
/// anywhere on it does the one thing it does.
///
/// # What a reader is told
///
/// Nothing of its own. It is a layout, and whatever is inside it is announced as it would be
/// anywhere else — which is why the controls in [`ItemActions`] need names even when the title
/// beside them makes their job obvious to somebody looking.
#[component]
pub fn Item(
    /// How it is framed.
    #[prop(default = ItemVariant::Default)]
    variant: ItemVariant,
    /// How much room it takes.
    #[prop(default = ItemSize::Md)]
    size: ItemSize,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the row holds.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, ItemStyle::CSS);
    install_stylesheet(PARTS_SHEET, ItemPartStyle::CSS);
    let context = ItemContext {
        description: RwSignal::new_local(false),
    };
    provide_local_context(context);
    let variants = ItemVariants { variant, size };
    let own = variant_attrs(variants.classes(), variants.data_attributes()).class_toggle(
        zgui::view::ClassName::new("zui-item--described"),
        move || context.has_description(),
    );

    view! {
        box(class = ItemStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// A stack of [`Item`]s read as one list.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! { ItemGroup {Item {ItemContent {ItemTitle {"report.pdf"}}}} }
/// # }
/// ```
#[component]
pub fn ItemGroup(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The items.
    children: Children,
) -> impl IntoView {
    install_stylesheet(GROUP_SHEET, ItemGroupStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::List));

    view! {
        box(class = ItemGroupStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}

/// A line between two [`Item`]s.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     ItemGroup {
///         Item {ItemContent {ItemTitle {"report.pdf"}}}
///         ItemSeparator()
///         Item {ItemContent {ItemTitle {"notes.txt"}}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn ItemSeparator(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(GROUP_SHEET, ItemGroupStyle::CSS);
    let own = Attrs::new().a11y_from(A11yBinding::new(Role::GenericContainer).hidden(true));

    view! { box(class = "zui-item__separator", {..own}, {..attrs}, class = class) }
}
