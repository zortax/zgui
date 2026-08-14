//! One setting: what it is called, what it means, and the control that changes it.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};

use crate::field::{FieldContentProps, FieldOrientation, FieldProps};
use crate::label::LabelProps;
use crate::settings::style;

/// What a control inside a [`SettingsItem`] reads to be named by the words beside it.
#[derive(Copy, Clone)]
pub struct SettingsItemContext {
    /// The words that name the control.
    label: NodeRef,
    /// The line under them, when the item was given one.
    description: NodeRef,
    /// The control itself.
    control: NodeRef,
    /// Whether the item was given a description at all.
    described: bool,
}

impl SettingsItemContext {
    /// The item the calling scope is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// The words that name the control.
    #[must_use]
    pub fn label(self) -> NodeRef {
        self.label
    }

    /// The line saying what the setting means.
    #[must_use]
    pub fn description(self) -> NodeRef {
        self.description
    }

    /// The control itself, for a caller who wants a press on the words to reach it.
    ///
    /// Bind it with `node_ref=` on the control, and pass the same handle to the item's `control`
    /// prop. Everything else about the item works without it.
    #[must_use]
    pub fn control(self) -> NodeRef {
        self.control
    }

    /// What a control has to carry to be named by the item it is inside.
    ///
    /// Spread onto any control with `{..use_settings_item_attrs()}`. A switch, a slider and a
    /// select carry no words of their own, so without this they are announced as "switch" and
    /// nothing else — the name is beside them on the screen and nowhere in the tree.
    #[must_use]
    pub fn attrs(self) -> Attrs {
        let mut semantics = A11yBinding::unspecified().labelled_by(self.label);
        if self.described {
            semantics = semantics.described_by(self.description);
        }
        Attrs::new().a11y_from(semantics)
    }
}

/// The item the calling scope is inside, when it is inside one.
///
/// `None` outside an item, which is an ordinary answer — the same control is used on its own
/// everywhere else.
#[must_use]
pub fn use_settings_item() -> Option<SettingsItemContext> {
    SettingsItemContext::current()
}

/// What a control has to carry to be named by the [`SettingsItem`] it is inside, or nothing at all
/// when it is inside none.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     SettingsItem(label = "Dark mode") {Switch({..use_settings_item_attrs()})}
/// }
/// # }
/// ```
#[must_use]
pub fn use_settings_item_attrs() -> Attrs {
    use_settings_item()
        .map(SettingsItemContext::attrs)
        .unwrap_or_default()
}

/// One setting: the words that name it, the line that explains it, and the control that changes
/// it.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::reactive::RwSignal;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// let charts = RwSignal::new_local(false);
/// view! {
///     SettingsGroup {
///         SettingsGroupLabel {"Port forwarding"}
///         SettingsItem(
///             label = "Traffic charts",
///             description = "Show sent and received traffic in the port forward dialog."
///         ) {
///             Switch(checked = charts, {..use_settings_item_attrs()})
///         }
///     }
/// }
/// # }
/// ```
///
/// # Why the words are props and the control is the children
///
/// A group holds items, so its children are items and its heading is a part. An item holds one
/// control, so its children are that control — anything at all, a switch, a field, a select, a
/// button, something the application wrote itself — and its words are props. Each part's children
/// are the thing that part is a container for.
///
/// # Naming the control
///
/// A control carries its own name, because an accessibility tree relates a control to the words
/// that name it rather than the words to their control. So the item publishes the handle and the
/// control takes it with `{..use_settings_item_attrs()}`. Without that the row still reads as a
/// named group, and the control inside it is announced as "switch" and nothing else.
///
/// A control that can be turned off takes `disabled` of its own as well: this prop fades the row
/// and stops it answering the pointer, and only the control can say it is unavailable.
#[component]
pub fn SettingsItem(
    /// What the setting is called.
    #[prop(into)]
    label: String,
    /// What a person needs to know in order to set it well.
    #[prop(into, optional)]
    description: Option<String>,
    /// Whether the setting is out of action, which fades the row and stops it answering.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// The control's own element, so a press on the words moves the keyboard to it.
    #[prop(optional)]
    control: Option<NodeRef>,
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The control, which sits at the trailing edge of the row.
    children: Children,
) -> impl IntoView {
    style::install();
    let context = SettingsItemContext {
        label: NodeRef::new(),
        description: NodeRef::new(),
        control: control.unwrap_or_default(),
        described: description.is_some(),
    };
    provide_local_context(context);

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-settings__item"), true)
        .a11y_from(A11yBinding::unspecified().labelled_by(context.label));

    // The item's own element rather than a `FieldDescription`, because the relation a control
    // takes from `attrs` names an element, and only an element this component built has a handle
    // to name.
    let said = description.map(|text| {
        view! {
            box(class = "zui-settings__item-description", node_ref = context.description) {
                {text}
            }
        }
    });

    view! {
        Field(
            orientation = FieldOrientation::Horizontal,
            disabled = disabled,
            {..own},
            {..attrs},
            class = class
        ) {
            FieldContent {
                Label(node_ref = context.label, control = context.control) {{label}}
                {said.into_view()}
            }
            box(class = "zui-settings__item-control") {{children.into_view_once()}}
        }
    }
}
