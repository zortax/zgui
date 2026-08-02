//! The component that draws an icon.

mod sheet;

pub use crate::view::sheet::IconStyle;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::vocab::PropValue;
use zgui::{component, view};

use crate::icon::{IconData, IconSize, IconVariants};

/// The style sheet every icon shares, installed once however many icons a program draws.
const SHEET: &str = "zui-icon";

/// Draws one icon.
///
/// The outline is a compile-time constant and the appearance is CSS: the box comes from
/// `--zui-icon-size`, which the `size` prop selects between, and the colour is the element's own
/// computed `color` — so an icon inside a button follows that button's text without being told,
/// and `.zui-alert--destructive .zui-icon { color: … }` re-colours one with no prop at all.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui_icons::set::mark::CHECK;
/// use zgui_ui_icons::prelude::*;
///
/// /// A row that says something went right.
/// #[component]
/// fn Done() -> impl IntoView {
///     view! {
///         row {
///             Icon(icon = CHECK, size = IconSize::Sm)
///             text {"Saved"}
///         }
///     }
/// }
/// ```
///
/// # What a reader is told
///
/// Nothing, unless `label` says otherwise. An icon beside a word repeats that word, and an
/// accessibility tree holding both is one a screen reader reads twice — so an icon is hidden from
/// the tree by default, and is a named image only when it carries the meaning on its own, as in an
/// icon-only button.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui_icons::prelude::*;
/// # use zgui_ui_icons::set::mark::CROSS;
/// # #[component]
/// # fn Close() -> impl IntoView {
/// view! { Icon(icon = CROSS, label = "Close") }
/// # }
/// ```
///
/// # An icon that changes
///
/// `icon` is a signal, so a disclosure chevron is one element rather than two branches:
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::reactive::RwSignal;
/// # use zgui::{component, view};
/// # use zgui_ui_icons::prelude::*;
/// # use zgui_ui_icons::set::chevron::{CHEVRON_DOWN, CHEVRON_UP};
/// # #[component]
/// # fn Disclosure() -> impl IntoView {
/// let open = RwSignal::new_local(false);
/// view! {
///     Icon(icon = Signal::derive_local(move || {
///         if open.get() { CHEVRON_UP } else { CHEVRON_DOWN }
///     }))
/// }
/// # }
/// ```
#[component]
pub fn Icon(
    /// Which icon to draw.
    #[prop(into)]
    icon: Signal<IconData, LocalStorage>,
    /// How large to draw it.
    #[prop(default = IconSize::Md)]
    size: IconSize,
    /// What the icon is called, when it carries meaning rather than decorating a word beside it.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after the icon's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView {
    install_stylesheet(SHEET, IconStyle::CSS);
    let variants = IconVariants { size };
    // A name and no name are two bindings rather than one with an empty string in it: an icon that
    // announces itself as `""` is worse than one that announces nothing, because a reader reads
    // the empty name instead of falling through to whatever does have one.
    let semantics = Attrs::new().a11y_from(match label {
        Some(text) => A11yBinding::new(Role::Image).label(text),
        None => A11yBinding::new(Role::Image).hidden(true),
    });

    view! {
        vector(
            class = variants.classes(),
            class = IconStyle::CLASS,
            attr:data-size = variants.data_attributes()[0].1,
            attr:data-icon = move || Some(icon.get().name().to_owned()),
            prop:d = move || PropValue::from(icon.get().path_data()),
            prop:viewBox = move || {
                let side = icon.get().view_box();
                PropValue::from(format!("0 0 {side} {side}").as_str())
            },
            {..semantics},
            {..attrs},
            class = class
        )
    }
}
