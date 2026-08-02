//! One choice out of several.

mod item;
mod style;

pub use crate::radio_group::item::{RadioGroupItem, RadioGroupItemProps};
pub use crate::radio_group::style::{RadioGroupStyle, RadioItemStyle};

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, view};
use zgui_ui_primitives::prelude::*;

/// What the radio group's rules are installed under.
pub(crate) const SHEET: &str = "zui-radio-group";

/// What an item's rules are installed under.
///
/// A name of its own rather than the group's, because installing under a taken name *replaces*
/// that sheet's text: one name shared by the group and its items is whichever of the two rendered
/// last, and the other renders unstyled.
pub(crate) const ITEM_SHEET: &str = "zui-radio-group-item";

/// What an item reads to know whether it is the chosen one, and how to become it.
#[derive(Copy, Clone)]
pub struct RadioContext {
    /// Which item's value is chosen, owned by whoever asked to own it.
    ///
    /// Nothing chosen is the empty string, which is not a value any item can have: an item is
    /// named by what choosing it reports, and a choice that reports nothing is not one.
    value: Controllable<String>,
    /// Whether the whole group is out of action.
    disabled: Signal<bool, LocalStorage>,
    /// The group's own element, which each item is related to for a reader.
    group: NodeRef,
}

impl RadioContext {
    /// The group an item is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Which value is chosen right now, or nothing when none of them is.
    #[must_use]
    pub fn chosen(&self) -> Option<String> {
        Some(self.value.get()).filter(|chosen| !chosen.is_empty())
    }

    /// Whether `value` is the chosen one.
    #[must_use]
    pub fn is_chosen(&self, value: &str) -> bool {
        self.chosen().is_some_and(|chosen| chosen == value)
    }

    /// Whether the whole group is out of action.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.disabled.get()
    }

    /// The group's own element.
    #[must_use]
    pub fn element(&self) -> NodeRef {
        self.group
    }

    /// Chooses `value`, and tells whoever asked to be told.
    pub fn choose(&self, value: &str) {
        self.value.set(value.to_owned());
    }
}

/// A set of choices, exactly one of which is taken.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Which way to be billed.
/// #[component]
/// fn Plan() -> impl IntoView {
///     let plan = RwSignal::new_local("monthly".to_owned());
///     view! {
///         RadioGroup(value = plan, label = "Billing") {
///             RadioGroupItem(value = "monthly")
///             RadioGroupItem(value = "yearly")
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// The group is one tab stop, not one per choice: <kbd>Tab</kbd> reaches whichever item is chosen,
/// and the arrow keys move between them — wrapping at the ends — choosing as they go, which is
/// what the authoring practices call for and what makes a radio group operable without ever
/// pressing space. That behaviour is [`RovingFocus`], not a key handler here.
#[component]
pub fn RadioGroup(
    /// Which value is chosen, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<String>,
    /// Which value starts chosen, when the group owns it itself.
    #[prop(into, optional)]
    default_value: Option<String>,
    /// Told whenever the choice changes.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<String>>,
    /// Whether any of it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Which way the choices are laid out, which decides which arrow keys move between them.
    #[prop(default = zgui_ui_primitives::Orientation::Vertical)]
    orientation: zgui_ui_primitives::Orientation,
    /// What the group is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after the group's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The choices.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, RadioGroupStyle::CSS);
    let group = NodeRef::new();
    let context = RadioContext {
        value: Controllable::new(value, default_value.unwrap_or_default(), on_change),
        disabled,
        group,
    };
    provide_local_context(context);

    let mut semantics = A11yBinding::new(Role::RadioGroup).orientation(match orientation {
        zgui_ui_primitives::Orientation::Horizontal => zgui::vocab::Orientation::Horizontal,
        // A group whose arrow keys work in both directions is laid out in neither in particular,
        // and reporting one of the two would be a claim about a layout nobody made.
        zgui_ui_primitives::Orientation::Vertical | zgui_ui_primitives::Orientation::Both => {
            zgui::vocab::Orientation::Vertical
        }
    });
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    // The orientation attribute is `RovingFocus`'s own, written from the same value, so it is not
    // repeated here: two writers of one attribute is one of them being wrong at some point.
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-radio-group"), true)
        .class_toggle(zgui::view::ClassName::new(RadioGroupStyle::CLASS), true)
        .a11y_from(semantics);

    view! {
        RovingFocus(orientation = orientation, element_ref = group, class = class, {..own}, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
