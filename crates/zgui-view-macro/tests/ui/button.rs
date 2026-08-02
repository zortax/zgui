//! The library's model component, compiled.
//!
//! It is a pass case rather than prose because every part of the authoring surface meets here:
//! a variants table, a scoped sheet, props with defaults and conversions, a forwarded bundle, a
//! reactive state binding, a typed listener that reads its payload and synthesises another event,
//! and a set of event kinds — which is a set of *kinds*, because two event constants are two
//! types and an array of them has no element type.

extern crate zgui_view as zgui;

use zgui_reactive::install;
use zgui_view::prelude::*;
use zgui_view::{
    AttrName, Attrs, ClassName, EventKind, ListenerOptions, ReactiveValue, Role, UiState,
};
use zgui_vocab::{Key, NamedKey};
use zgui_view_macro::{component, style, variants, view};

variants! {
    /// The visual variants of [`Button`].
    pub ButtonVariants {
        base: "zui-btn",
        variant: {
            Default => "zui-btn--default",
            Destructive => "zui-btn--destructive",
            Outline => "zui-btn--outline",
            Ghost => "zui-btn--ghost",
        } = Default,
        size: { Sm => "zui-btn--sm", Md => "", Lg => "zui-btn--lg" } = Md,
    }
}

style! { pub ButtonStyle =>
    ":scope { display: inline-flex; align-items: center; justify-content: center; }"
    ":scope:focus-visible { outline: 2px solid var(--zui-ring); outline-offset: 2px; }"
    ":scope[data-disabled] { opacity: .5; pointer-events: none; }"
}

/// A button.
#[component]
pub fn Button(
    /// How it looks.
    #[prop(default = ButtonVariant::Default)]
    variant: ButtonVariant,
    /// How big it is.
    #[prop(default = ButtonSize::Md)]
    size: ButtonSize,
    /// Whether it can be pressed.
    #[prop(into, default = ReactiveValue::Constant(false))]
    disabled: ReactiveValue<bool>,
    /// Classes merged after the button's own.
    #[prop(into, optional)]
    class: Classes,
    /// What the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// The label.
    children: Children,
) -> impl IntoView {
    let variants = ButtonVariants { variant, size };
    let activates_on: &[EventKind] = &[events::CLICK.kind(), events::KEY_DOWN.kind()];
    let is_disabled = disabled.clone();
    let marks_disabled = disabled.clone();

    let own = Attrs::new()
        .classes_from(variants.classes())
        .class_toggle(ClassName::new(ButtonStyle::CLASS), true)
        .state(UiState::DISABLED, move || is_disabled.get())
        .attribute(AttrName::new("data-variant"), variants.data_attributes()[0].1)
        .listener(events::KEY_DOWN, ListenerOptions::DEFAULT, move |ev| {
            // The payload is the event's own, so this needs no downcast and no accessor.
            if ev.key == Key::Named(NamedKey::Enter) && !ev.repeat {
                ev.synthesize(events::CLICK);
            }
        })
        .a11y_from(
            A11yBinding::with_role(Role::Button).disabled(move || marks_disabled.get()),
        );

    let _forwarded = own.merged(attrs).classes_from(class);
    assert_eq!(activates_on.len(), 2);
    view! { {children.into_view_once()} }
}

fn main() {
    install().ok();
    let _ = view! {
        Button(
            variant = ButtonVariant::Outline,
            class = "w-full",
            attr:data-testid = "save",
            on:click:stop = move |_| {}
        ) {
            "Save"
        }
    };

    // The same call written the way it reads: the value ends at a `>` that is glued to the `<`
    // of what follows it, which is what rustc hands the macro for `…=x><…` and `…=x></…`.
    let outline = ButtonVariant::Outline;
    let _ = view! { Button(variant = outline, a11y:label = "Save") {Button {"Save"}} };
}
