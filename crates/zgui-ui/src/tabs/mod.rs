//! One surface showing one of several panels at a time.

mod content;
mod list;
mod style;
mod trigger;

pub use crate::tabs::content::{TabsContent, TabsContentProps};
pub use crate::tabs::list::{TabsList, TabsListProps, TabsListVariant, TabsListVariants};
pub use crate::tabs::style::TabsStyle;
pub use crate::tabs::trigger::{TabsTrigger, TabsTriggerProps};

use std::collections::BTreeMap;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, UnsyncCallback};
use zgui::{component, view};
use zgui_ui_primitives::{Binding, Controllable, Orientation};

/// What the tabs' rules are installed under.
pub(crate) const SHEET: &str = "zui-tabs";

/// Whether arrowing to a tab shows it, or only moves to it.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum TabsActivation {
    /// Arrowing to a tab shows its panel straight away.
    ///
    /// Right when each panel is already there and cheap to show, which is most of the time.
    #[default]
    Automatic,
    /// Arrowing moves the focus and shows nothing until <kbd>Enter</kbd> or <kbd>Space</kbd>.
    ///
    /// Right when showing a panel costs something a user did not ask for — a request, a large
    /// document, a running computation.
    Manual,
}

impl TabsActivation {
    /// How this is written as an attribute value.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Manual => "manual",
        }
    }
}

/// What a tab and a panel read to know whether they are the ones showing.
#[derive(Copy, Clone)]
pub struct TabsContext {
    /// Which tab is showing.
    value: Controllable<String>,
    /// Which way the tabs run, which decides which arrow keys move between them.
    orientation: Orientation,
    /// Whether arrowing to a tab shows it.
    activation: TabsActivation,
    /// One handle per tab, by value, so a panel can name the tab that labels it.
    triggers: RwSignal<BTreeMap<String, NodeRef>, LocalStorage>,
    /// One handle per panel, by value, so a tab can name what it controls.
    panels: RwSignal<BTreeMap<String, NodeRef>, LocalStorage>,
}

impl TabsContext {
    /// The tabs the calling scope is inside, when it is inside any.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Which tab is showing.
    #[must_use]
    pub fn value(self) -> String {
        self.value.get()
    }

    /// Whether the tab called `value` is the one showing.
    #[must_use]
    pub fn is_selected(self, value: &str) -> bool {
        self.value.get() == value
    }

    /// Shows the panel called `value`.
    pub fn select(self, value: &str) {
        self.value.set(value.to_owned());
    }

    /// Which way the tabs run.
    #[must_use]
    pub fn orientation(self) -> Orientation {
        self.orientation
    }

    /// Whether arrowing to a tab shows it.
    #[must_use]
    pub fn activation(self) -> TabsActivation {
        self.activation
    }

    /// The handle the tab called `value` binds, minting one the first time it is asked for.
    ///
    /// Held by the tab set rather than by either part, because the two parts need *each other's*
    /// element and are built in either order — and a panel that is not mounted still has to be
    /// nameable by the tab that would show it.
    #[must_use]
    pub fn trigger_of(self, value: &str) -> NodeRef {
        Self::entry(self.triggers, value)
    }

    /// The handle the panel called `value` binds.
    #[must_use]
    pub fn panel_of(self, value: &str) -> NodeRef {
        Self::entry(self.panels, value)
    }

    /// One map's entry for `value`, minted on first use.
    fn entry(map: RwSignal<BTreeMap<String, NodeRef>, LocalStorage>, value: &str) -> NodeRef {
        if let Some(found) = map.with_untracked(|map| map.get(value).copied()) {
            return found;
        }
        let node = NodeRef::new();
        map.update(|map| {
            map.insert(value.to_owned(), node);
        });
        node
    }
}

/// One surface showing one of several panels at a time.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// An account page with two panels.
/// #[component]
/// fn Account() -> impl IntoView {
///     view! {
///         Tabs(default_value = "profile", label = "Account") {
///             TabsList {
///                 TabsTrigger(value = "profile") {"Profile"}
///                 TabsTrigger(value = "billing") {"Billing"}
///             }
///             TabsContent(value = "profile") {text {"Your name and picture."}}
///             TabsContent(value = "billing") {text {"Cards and invoices."}}
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// One tab stop for the whole strip, as the authoring practices ask. The arrow keys for the strip's
/// own orientation move between the tabs and wrap at the ends, <kbd>Home</kbd> and <kbd>End</kbd>
/// go to the first and last, and <kbd>Tab</kbd> leaves the strip for the panel. The arrows for the
/// *other* axis are left alone, so a vertical strip beside a scrolling panel does not swallow the
/// keys that scroll it.
///
/// Whether arrowing also shows a panel is [`TabsActivation`], and it defaults to showing it.
#[component]
pub fn Tabs(
    /// Which tab is showing, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<String>,
    /// Which one starts showing, when the tab set owns that itself.
    #[prop(into, default = String::new())]
    default_value: String,
    /// Told whenever the showing tab changes, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<String>>,
    /// Which way the strip runs.
    #[prop(default = Orientation::Horizontal)]
    orientation: Orientation,
    /// Whether arrowing to a tab shows its panel.
    #[prop(default = TabsActivation::Automatic)]
    activation: TabsActivation,
    /// What the whole set is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Where to record this component's own element.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the tab set's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The strip and the panels.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, TabsStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let context = TabsContext {
        value: Controllable::new(value, default_value, on_change),
        orientation,
        activation,
        triggers: RwSignal::new_local(BTreeMap::new()),
        panels: RwSignal::new_local(BTreeMap::new()),
    };
    provide_local_context(context);

    let mut semantics = A11yBinding::new(Role::Group);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-tabs"), true)
        .attribute(
            zgui::view::AttrName::new("data-orientation"),
            orientation.name(),
        )
        .attribute(
            zgui::view::AttrName::new("data-activation"),
            activation.name(),
        )
        .a11y_from(semantics);

    view! {
        column(node_ref = element, class = TabsStyle::CLASS, {..own}, {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
