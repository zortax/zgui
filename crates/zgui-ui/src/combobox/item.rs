//! One option of a combobox, mounted only while it survives the filter.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, StoredValue};
use zgui::view::AttrName;
use zgui::vocab::UiState;
use zgui::{component, view};
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::CHECK;

use crate::combobox::SHEET;
use crate::combobox::style::ComboboxStyle;
use crate::listbox::{Listbox, ListboxOption, use_listbox_option};
use crate::select::{SHEET as SELECT_SHEET, SelectStyle};

/// One option of a [`Combobox`](crate::Combobox).
///
/// While the filter excludes it, nothing is mounted and nothing is registered — so it is not a row
/// a reader meets, not somewhere an arrow key lands, and not an option the count of what is left
/// includes.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     Combobox {ComboboxContent {
///         ComboboxItem(value = "gb") {"United Kingdom"}
///         ComboboxItem(value = "us", disabled = true) {"United States"}
///     }}
/// }
/// # }
/// ```
#[component]
pub fn ComboboxItem(
    /// What choosing it reports.
    #[prop(into)]
    value: String,
    /// What it reads as and is searched by.
    ///
    /// The value unless this says otherwise, and it is written rather than read off the element
    /// for a reason that cannot be worked around: an option the filter has excluded is **not
    /// mounted**, so there is no element to read the text off — and an option that could not be
    /// searched while it was hidden could never come back.
    #[prop(into, optional)]
    text: Option<String>,
    /// Whether it can be chosen.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// Told when it is chosen.
    #[prop(optional)]
    on_select: Option<zgui::reactive::UnsyncCallback<()>>,
    /// Classes merged after the option's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it shows.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, ComboboxStyle::CSS);
    // A combobox's rows are a select's rows, so they are drawn by a select's rules — which have to
    // be installed here as well. A component that borrowed the classes and not the sheet would be
    // styled only on a page that happened to hold a select too.
    install_stylesheet(SELECT_SHEET, SelectStyle::CSS);
    let listbox = Listbox::current();
    let reads_as = text.unwrap_or_else(|| value.clone());
    let survives = {
        let reads_as = reads_as.clone();
        move || listbox.is_none_or(|listbox| listbox.matches(&reads_as))
    };

    let held = StoredValue::new_local((value, reads_as, on_select, class, attrs, children));

    view! {
        Show(when = survives, fallback = || ()) {
            {move || {
                let (value, reads_as, on_select, class, attrs, children) = held.get_value();
                view! {
                    ComboboxOption(
                        value = value,
                        text = reads_as,
                        disabled = disabled,
                        on_select = on_select,
                        class = class,
                        {..attrs}
                    ) {
                        {children.view()}
                    }
                }
            }}
        }
    }
}

/// The option itself, mounted only while its text survives the filter.
///
/// Its own component because registration has to happen in a scope that goes away when the filter
/// stops matching: a registration in the parent's scope would outlive the row it stands for, and
/// the arrow keys would walk options nobody can see.
#[component]
fn ComboboxOption(
    /// What choosing it reports.
    value: String,
    /// What it reads as.
    text: String,
    /// Whether it can be chosen.
    disabled: Signal<bool, LocalStorage>,
    /// Told when it is chosen.
    on_select: Option<zgui::reactive::UnsyncCallback<()>>,
    /// Classes merged after the option's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it shows.
    children: Children,
) -> impl IntoView {
    let node = NodeRef::new();
    let mut option = ListboxOption::new(node, value.clone(), text, disabled);
    if let Some(on_select) = on_select {
        option = option.on_select(on_select);
    }
    let registered = use_listbox_option(option);

    let value = StoredValue::new_local(value);
    let chosen =
        move || registered.is_some_and(|(listbox, _)| listbox.is_chosen(&value.get_value()));
    let active = move || registered.is_some_and(|(listbox, id)| listbox.active() == Some(id));

    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-select__item"), true)
        .attribute(AttrName::new("data-active"), move || {
            active().then(|| "true".to_owned())
        })
        .attribute(AttrName::new("data-disabled"), move || {
            disabled.get().then(|| "true".to_owned())
        })
        .state(UiState::CHECKED, chosen)
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(
            A11yBinding::new(Role::ListBoxOption)
                .selected(chosen)
                .disabled(move || disabled.get()),
        );

    view! {
        box(
            node_ref = node,
            on:click = move |_| {
                if let Some((listbox, id)) = registered {
                    listbox.take(id);
                }
            },
            on:pointer_enter = move |_| {
                if !disabled.get_untracked()
                    && let Some((listbox, id)) = registered
                {
                    listbox.set_active(Some(id));
                }
            },
            {..own},
            {..attrs},
            class = class
        ) {
            box(class = "zui-select__indicator") {
                Show(when = chosen, fallback = || ()) {
                    Icon(icon = CHECK)
                }
            }
            {children.into_view_once()}
        }
    }
}
