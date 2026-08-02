//! A button that stays pressed.

mod style;

pub use crate::toggle::style::ToggleStyle;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::vocab::UiState;
use zgui::{component, variants, view};
use zgui_ui_primitives::{Binding, Controllable};

use crate::support::variant_attrs;

/// What the toggle's rules are installed under.
pub(crate) const SHEET: &str = "zui-toggle";

variants! {
    /// The axes a [`Toggle`] varies along.
    pub ToggleVariants {
        base: "zui-toggle",
        variant: { Default => "", Outline => "zui-toggle--outline" } = Default,
        size: { Sm => "zui-toggle--sm", Md => "", Lg => "zui-toggle--lg" } = Md,
    }
}

/// A button with two positions: pressed, and not.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// A formatting control that stays on.
/// #[component]
/// fn Bold() -> impl IntoView {
///     let bold = RwSignal::new_local(false);
///     view! { Toggle(pressed = bold, label = "Bold") {"B"} }
/// }
/// ```
///
/// # A toggle and a switch
///
/// A switch turns something on; a toggle *is* on. The difference shows in what a reader is told: a
/// toggle is a button that reports whether it is pressed, so it is announced as the control it
/// looks like, and it belongs in a toolbar beside other buttons rather than in a list of settings.
///
/// # Keyboard
///
/// <kbd>Space</kbd> and <kbd>Enter</kbd>, through the framework's own activation of what has
/// focus.
#[component]
pub fn Toggle(
    /// Whether it is pressed, when the caller holds it.
    ///
    /// A writable signal is written back to; a value the caller computes needs
    /// [`Binding::controlled`].
    #[prop(into, optional)]
    pressed: Binding<bool>,
    /// Whether it starts pressed, when it owns that itself.
    #[prop(default = false)]
    default_pressed: bool,
    /// Told whenever it changes, whoever owns it.
    #[prop(optional)]
    on_change: Option<UnsyncCallback<bool>>,
    /// How it looks.
    #[prop(default = ToggleVariant::Default)]
    variant: ToggleVariant,
    /// How big it is.
    #[prop(default = ToggleSize::Md)]
    size: ToggleSize,
    /// Whether it can be operated.
    #[prop(into, default = Signal::stored_local(false))]
    disabled: Signal<bool, LocalStorage>,
    /// What it is called, for a reader, when its content is a mark rather than a word.
    #[prop(into, optional)]
    label: Option<String>,
    /// Where to record this component's own element, for relating it to something else.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the toggle's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What it shows.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, ToggleStyle::CSS);
    let element = node_ref.unwrap_or_default();
    let value = Controllable::new(pressed, default_pressed, on_change);
    let variants = ToggleVariants { variant, size };

    let mut semantics = A11yBinding::new(Role::Button)
        .toggled_on(move || value.get())
        .disabled(move || disabled.get());
    if let Some(text) = label {
        semantics = semantics.label(text);
    }

    let own = variant_attrs(variants.classes(), variants.data_attributes())
        .attribute(zgui::view::AttrName::new("data-state"), move || {
            Some(if value.get() { "on" } else { "off" }.to_owned())
        })
        .state(UiState::CHECKED, move || value.get())
        .state(UiState::DISABLED, move || disabled.get())
        .a11y_from(semantics);

    view! {
        control(
            node_ref = element,
            class = ToggleStyle::CLASS,
            tabindex = {Focus::Sequential},
            on:click = move |_| {
                if !disabled.get_untracked() {
                    value.toggle();
                }
            },
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
        }
    }
}
