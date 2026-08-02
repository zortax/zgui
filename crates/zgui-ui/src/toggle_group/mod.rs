//! A row of toggles that know about one another.

mod item;
mod style;

pub use crate::toggle_group::item::{ToggleGroupItem, ToggleGroupItemProps};
pub use crate::toggle_group::style::ToggleGroupStyle;

use std::collections::BTreeSet;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, view};
use zgui_ui_primitives::prelude::*;

use crate::toggle::{ToggleSize, ToggleVariant};

/// What the group's rules are installed under.
pub(crate) const SHEET: &str = "zui-toggle-group";

/// How many of a group's toggles may be on at once.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub enum ToggleSelection {
    /// At most one, and pressing another turns the first off. A text alignment picker.
    #[default]
    Single,
    /// Any number of them. A set of formatting marks.
    Multiple,
}

/// What an item reads to know whether it is on, and how to change that.
#[derive(Copy, Clone)]
pub struct ToggleGroupContext {
    /// Which values are on, owned by whoever asked to own them.
    values: Controllable<Vec<String>>,
    /// How many may be on at once.
    selection: ToggleSelection,
    /// Whether the whole group is out of action.
    disabled: Signal<bool, LocalStorage>,
    /// How the items look, unless one of them says otherwise.
    variant: ToggleVariant,
    /// How big the items are, unless one of them says otherwise.
    size: ToggleSize,
}

impl ToggleGroupContext {
    /// The group an item is inside, when it is inside one.
    #[must_use]
    pub fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Everything that is on right now.
    #[must_use]
    pub fn on(&self) -> BTreeSet<String> {
        self.values.get().into_iter().collect()
    }

    /// Whether `value` is on.
    #[must_use]
    pub fn is_on(&self, value: &str) -> bool {
        self.on().contains(value)
    }

    /// Whether the whole group is out of action.
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.disabled.get()
    }

    /// How many of the group may be on at once.
    #[must_use]
    pub fn selection(&self) -> ToggleSelection {
        self.selection
    }

    /// How the group's items look, for an item that did not ask for a look of its own.
    #[must_use]
    pub fn variant(&self) -> ToggleVariant {
        self.variant
    }

    /// How big the group's items are, for an item that did not ask for a size of its own.
    #[must_use]
    pub fn size(&self) -> ToggleSize {
        self.size
    }

    /// Turns `value` on if it was off, and off if it was on.
    ///
    /// In a single-selection group turning one on turns every other one off, which is the whole
    /// difference between the two kinds — and pressing the one that is already on turns it off,
    /// leaving nothing selected. A group that refused that would be a radio group wearing a
    /// toolbar's clothes.
    pub fn toggle(&self, value: &str) {
        let mut next = self.on();
        if next.contains(value) {
            next.remove(value);
        } else {
            if self.selection == ToggleSelection::Single {
                next.clear();
            }
            next.insert(value.to_owned());
        }
        self.values.set(next.into_iter().collect());
    }
}

/// A row of toggles operated as one control.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Which way the text lines up: exactly one answer.
/// #[component]
/// fn Align() -> impl IntoView {
///     let align = RwSignal::new_local(vec!["left".to_owned()]);
///     view! {
///         ToggleGroup(value = align, selection = ToggleSelection::Single, label = "Alignment") {
///             ToggleGroupItem(value = "left", label = "Left") {"L"}
///             ToggleGroupItem(value = "center", label = "Centre") {"C"}
///             ToggleGroupItem(value = "right", label = "Right") {"R"}
///         }
///     }
/// }
/// ```
///
/// # Keyboard
///
/// One tab stop for the group, and the arrow keys move between its items — the toolbar pattern,
/// which is what a group of buttons that belong together is. Space and Enter press whichever item
/// the arrows landed on; unlike a radio group, arrowing does not press anything on the way.
///
/// # The seam
///
/// `spacing` is how far apart the items sit, in spacing steps, and it defaults to none: the items
/// meet, the ends of the strip are the only rounded corners, and two outlined items share one
/// border instead of drawing two. Anything above zero opens the strip up into separate controls
/// with their own corners.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     ToggleGroup(variant = ToggleVariant::Outline, spacing = 1.0, label = "Formatting") {
///         ToggleGroupItem(value = "bold", label = "Bold") {"B"}
///         ToggleGroupItem(value = "italic", label = "Italic") {"I"}
///     }
/// }
/// # }
/// ```
///
/// `variant` and `size` are the group's, and an item takes them unless it names its own — so a
/// strip is described once rather than once per item.
#[component]
pub fn ToggleGroup(
    /// Which values are on, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<Vec<String>>,
    /// Which values start on, when the group owns that itself.
    #[prop(optional)]
    default_value: Option<Vec<String>>,
    /// Told whenever the set changes.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<Vec<String>>>,
    /// How many may be on at once.
    #[prop(default = ToggleSelection::Single)]
    selection: ToggleSelection,
    /// How the items look, unless one of them says otherwise.
    #[prop(default = ToggleVariant::Default)]
    variant: ToggleVariant,
    /// How big the items are, unless one of them says otherwise.
    #[prop(default = ToggleSize::Md)]
    size: ToggleSize,
    /// How far apart the items sit, in spacing steps. None of it joins them into one strip.
    #[prop(default = 0.0)]
    spacing: f32,
    /// Whether any of it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Which way the items are laid out, which decides which arrow keys move between them.
    #[prop(default = Orientation::Horizontal)]
    orientation: Orientation,
    /// What the group is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after the group's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The items.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, ToggleGroupStyle::CSS);
    let context = ToggleGroupContext {
        values: Controllable::new(value, default_value.unwrap_or_default(), on_change),
        selection,
        disabled,
        variant,
        size,
    };
    provide_local_context(context);

    let mut semantics = A11yBinding::new(Role::Toolbar);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    // The spacing reaches the sheet twice: as the gap itself, and as an attribute the seam rules
    // select on. A gap of nothing and a gap of one step are two different shapes, not two values
    // of one, so the difference has to be something a selector can see.
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-toggle-group"), true)
        .class_toggle(zgui::view::ClassName::new(ToggleGroupStyle::CLASS), true)
        .attribute(zgui::view::AttrName::new("data-spacing"), move || {
            Some(format!("{spacing}"))
        })
        .attribute(zgui::view::AttrName::new("data-variant"), move || {
            Some(variant.name().to_string())
        })
        .custom_property(
            zgui::view::CustomPropertyName::new("zui-toggle-group-gap"),
            move || Some(format!("{spacing}")),
        )
        .a11y_from(semantics);

    view! {
        RovingFocus(orientation = orientation, class = class, {..own}, {..attrs}) {
            {children.into_view_once()}
        }
    }
}
