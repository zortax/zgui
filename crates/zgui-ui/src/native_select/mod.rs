//! The plain chooser: one closed control, and a list of words to pick from.

mod parts;
mod style;

pub use crate::native_select::parts::{
    NativeSelectOptGroup, NativeSelectOptGroupProps, NativeSelectOption, NativeSelectOptionProps,
};
pub use crate::native_select::style::NativeSelectStyle;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal, StoredValue, UnsyncCallback};
use zgui::view::AttrName;
use zgui::vocab::{HasPopup, UiState};
use zgui::{component, variants, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_DOWN;
use zgui_ui_primitives::{Align, Binding, Placement, Side};

use crate::listbox::{Listbox, ListboxCatalogueOfProps, ListboxOption};
use crate::overlay::{AnchoredSurfaceProps, OverlayState};
use crate::support::variant_attrs;

/// What the chooser's rules are installed under.
pub(crate) const SHEET: &str = "zui-native-select";

variants! {
    /// The axes a [`NativeSelect`] varies along.
    pub NativeSelectVariants {
        base: "zui-native-select",
        size: { Sm => "zui-native-select--sm", Md => "" } = Md,
    }
}

/// What a closed chooser shows before anything has been chosen.
///
/// The platform's own select shows its first option from the moment it exists, and this component
/// keeps that promise: the first option to describe itself is written down here, once, and the
/// control wears its text until a real choice replaces it. Kept as text rather than as the option,
/// because the options are unmounted and rebuilt as the list opens and closes, and a handle to the
/// first build would go dark exactly when the list is open.
#[derive(Copy, Clone)]
pub(crate) struct FirstShown {
    /// Whether some option has already spoken for the slot, settled at build time so the slot
    /// belongs to tree order rather than to whichever element happens to bind first.
    claimed: RwSignal<bool, LocalStorage>,
    /// What that option reads as, once its element exists to be read.
    text: RwSignal<Option<String>, LocalStorage>,
}

impl FirstShown {
    /// An empty slot.
    fn new() -> Self {
        Self {
            claimed: RwSignal::new_local(false),
            text: RwSignal::new_local(None),
        }
    }

    /// Publishes this to every scope below, and hands it back.
    fn provide(self) -> Self {
        provide_local_context(self);
        self
    }

    /// The slot the calling scope is inside, when it is inside one.
    pub(crate) fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// Offers one option for the slot; only the first offer ever counts.
    ///
    /// The text is learned the way [`ListboxLabels`](crate::listbox::ListboxLabels) learns: an
    /// option reads as the text it renders, which nothing can answer until its element exists, so
    /// the claim is made now and the reading waits for the handle to bind.
    pub(crate) fn offer(&self, option: ListboxOption) {
        if self.claimed.get_untracked() {
            return;
        }
        self.claimed.set(true);
        let text = self.text;
        let watching = zgui::reactive::RenderEffect::new(move |_| {
            if option.node().get().is_none() {
                return;
            }
            let shown = option.text();
            if shown.is_empty() || text.with_untracked(Option::is_some) {
                return;
            }
            text.set(Some(shown));
        });
        // Stored rather than dropped: an effect runs only for as long as something holds it, and
        // one dropped here would run once, before the element it waits for exists, and never again.
        StoredValue::new_local(watching);
    }

    /// What the slot reads as, once anything has been learned.
    fn shown(&self) -> Option<String> {
        self.text.get()
    }
}

/// The plain chooser: a control showing the choice, and a list of words under it.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Which way round to print.
/// #[component]
/// fn Orientation() -> impl IntoView {
///     view! {
///         NativeSelect {
///             NativeSelectOption(value = "portrait") {"Portrait"}
///             NativeSelectOption(value = "landscape") {"Landscape"}
///         }
///     }
/// }
/// ```
///
/// # This one and [`Select`](crate::Select)
///
/// [`Select`](crate::Select) is composed out of parts, so an option can hold a picture, two lines
/// of text or a keyboard shortcut, and the trigger can hold anything at all. This one cannot: an
/// option is a piece of text, and the whole chooser is written as one component with the options
/// inside it — which is the right amount of machinery inside a dense row, a table cell or a
/// toolbar where *choose one of four words* is the entire job. Under the surface the two share one
/// keyboard model and one listbox, so they cannot drift apart on what the down arrow does.
///
/// # What is shown while nothing is chosen
///
/// The first option's text, the way the platform's own select would — or `placeholder` when one is
/// given, which reads as a hint rather than as a choice.
///
/// # What a reader is told
///
/// That it is a chooser with a list, whether the list is showing, and which option the arrow keys
/// are on. Name it with a [`Label`](crate::Label) or with `a11y:label`.
#[component]
pub fn NativeSelect(
    /// Which value is chosen, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    value: Binding<String>,
    /// Which value starts chosen, when the chooser owns it itself.
    #[prop(into, optional)]
    default_value: Option<String>,
    /// Told whenever the choice changes, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<String>>,
    /// What to show while nothing is chosen, instead of the first option's text.
    #[prop(into, optional)]
    placeholder: Option<String>,
    /// How big it is.
    #[prop(default = NativeSelectSize::Md)]
    size: NativeSelectSize,
    /// Whether it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Whether leaving the choice as it is would be wrong, which reddens its border and its ring.
    #[prop(into, default = Signal::stored_local(false))]
    invalid: Signal<bool, LocalStorage>,
    /// Where to record the chooser's own element, for relating it to a label.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the chooser's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The options, and the groups they fall into.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, NativeSelectStyle::CSS);
    let surface = OverlayState::uncontrolled(false, None);
    let listbox = Listbox::new(surface, value, default_value, on_change).provide();
    let first = FirstShown::new().provide();
    // The overlay's own handle rather than a fresh one, because the list is anchored to whatever
    // that handle binds — a caller's ref is honoured the same way a date picker honours one.
    let element = node_ref.unwrap_or_else(|| surface.trigger());
    // Held rather than moved: the options are written once and built in two places — the open
    // list, and the hidden catalogue that teaches the closed control what its value reads as.
    let children = StoredValue::new_local(children);

    let hint = placeholder.unwrap_or_default();
    let empty = {
        let listbox = listbox;
        move || listbox.chosen_text().is_none() && first.shown().is_none()
    };
    let shown = {
        let hint = hint.clone();
        move || {
            listbox
                .chosen_text()
                .or_else(|| first.shown())
                .unwrap_or_else(|| hint.clone())
        }
    };

    let on_key_down = handler(
        events::KEY_DOWN,
        move |ev: &mut EventCx<'_, events::KeyDown>| {
            if disabled.get_untracked() {
                return;
            }
            if listbox.handle(&ev.key) {
                // Only when it meant something here. Tab, and every key this control does not
                // claim, has to reach whatever is around it.
                ev.prevent_default();
                ev.stop_propagation();
            }
        },
    );

    let semantics = A11yBinding::new(Role::ComboBox)
        .has_popup(HasPopup::Listbox)
        .expanded(move || surface.is_open())
        .controls(surface.content())
        .disabled(move || disabled.get())
        .step(move |a11y| {
            if invalid.get() {
                a11y.invalid(zgui::vocab::Invalid::True)
            } else {
                a11y
            }
        })
        .active_descendant(move || {
            zgui::vocab::NodeId(
                listbox
                    .active_node()
                    .get()
                    .map_or(0, zgui::view::NodeId::as_u64),
            )
        });

    let variants = NativeSelectVariants { size };
    let own = variant_attrs(variants.classes(), variants.data_attributes())
        .attribute(AttrName::new("data-state"), move || {
            Some(surface.state_name().to_owned())
        })
        .state(UiState::DISABLED, move || disabled.get())
        .state(UiState::INVALID, move || invalid.get())
        .a11y_from(semantics);

    let value_attrs = Attrs::new()
        .class_toggle(
            zgui::view::ClassName::new("zui-native-select__placeholder"),
            empty.clone(),
        )
        .state(UiState::PLACEHOLDER_SHOWN, empty);

    let trigger = view! {
        control(
            node_ref = element,
            class = NativeSelectStyle::CLASS,
            tabindex = {Focus::Sequential},
            on:click = move |_| {
                if !disabled.get_untracked() {
                    let was_open = surface.is_open_untracked();
                    surface.toggle();
                    if !was_open {
                        listbox.highlight_chosen();
                    }
                }
            },
            on:key_down = on_key_down,
            {..own},
            {..attrs},
            class = class
        ) {
            text(class = "zui-native-select__value", {..value_attrs}) {{shown}}
            Icon(icon = CHEVRON_DOWN, class = "zui-native-select__mark")
        }
    };
    // The list is portalled and contributes nothing where it is written; the catalogue builds the
    // options out of sight, exactly while the list does not, purely so that they say what their
    // values read as — a closed chooser has no options, and its options are the only thing that
    // knows their text. See [`SelectContent`](crate::SelectContent) for the long form of why.
    let list = view! {
        AnchoredSurface(
            state = surface,
            placement = {Signal::stored_local(Placement::new(Side::Bottom, Align::Start))},
            role = {Role::ListBox},
            class = "zui-native-select__list"
        ) {
            {children.get_value().view()}
        }
    };
    let described = view! {
        if move || !surface.is_open() {
            ListboxCatalogueOf {{children.get_value().view()}}
        } else {}
    };

    (trigger.into_view(), list.into_view(), described.into_view())
}
