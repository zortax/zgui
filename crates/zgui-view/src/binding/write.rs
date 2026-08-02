//! One binding per kind of thing an element can be told.

use zgui_interned::{AttrName, ClassName, CustomPropertyName, Ident};
use zgui_vocab::{PropKey, PropValue, Semantics, UiState};

use crate::binding::a11y::A11yBinding;
use crate::binding::effect::Binding;
use crate::cx::BuildCx;
use crate::id::NodeId;
use crate::value::IntoReactiveValue;

/// Binds one attribute, which is visible to selector matching.
///
/// `None` removes the attribute rather than setting it to an empty value.
pub fn bind_attribute<M>(
    cx: &BuildCx<'_>,
    el: NodeId,
    name: AttrName,
    value: impl IntoReactiveValue<Option<String>, M>,
) -> Binding {
    let dom = cx.dom_handle();
    Binding::new(cx, value.into_reactive_value(), move |value| {
        dom.set_attribute(el, name, value.as_deref());
    })
}

/// Binds one class on or off, leaving the rest of the list alone.
pub fn bind_class<M>(
    cx: &BuildCx<'_>,
    el: NodeId,
    class: ClassName,
    on: impl IntoReactiveValue<bool, M>,
) -> Binding {
    let dom = cx.dom_handle();
    Binding::new(cx, on.into_reactive_value(), move |on| {
        dom.toggle_class(el, class, *on);
    })
}

/// Binds one inline style declaration.
pub fn bind_style_property<M>(
    cx: &BuildCx<'_>,
    el: NodeId,
    property: impl Into<String>,
    value: impl IntoReactiveValue<Option<String>, M>,
) -> Binding {
    let dom = cx.dom_handle();
    let property = property.into();
    Binding::new(cx, value.into_reactive_value(), move |value| {
        dom.set_style_property(el, &property, value.as_deref());
    })
}

/// Binds one custom property.
pub fn bind_custom_property<M>(
    cx: &BuildCx<'_>,
    el: NodeId,
    property: CustomPropertyName,
    value: impl IntoReactiveValue<Option<String>, M>,
) -> Binding {
    let dom = cx.dom_handle();
    Binding::new(cx, value.into_reactive_value(), move |value| {
        dom.set_custom_property(el, property, value.as_deref());
    })
}

/// Binds one interaction state on or off.
///
/// Only the states a view may assert about its own element go through here. Hover, focus and
/// activation are computed by the input system, and a view that could assert them would be lying
/// to it.
pub fn bind_ui_state<M>(
    cx: &BuildCx<'_>,
    el: NodeId,
    state: UiState,
    on: impl IntoReactiveValue<bool, M>,
) -> Binding {
    debug_assert!(
        UiState::AUTHOR_SETTABLE.contains(state),
        "{state:?} is computed by the framework and cannot be asserted by a view"
    );
    let dom = cx.dom_handle();
    Binding::new(cx, on.into_reactive_value(), move |on| {
        dom.set_ui_state(el, state, *on);
    })
}

/// Binds one author-defined state on or off.
///
/// An author-defined state matches `:state(name)` in a selector, and is how a view expresses a
/// condition the closed set of interaction states does not name.
pub fn bind_custom_state<M>(
    cx: &BuildCx<'_>,
    el: NodeId,
    name: Ident,
    on: impl IntoReactiveValue<bool, M>,
) -> Binding {
    let dom = cx.dom_handle();
    Binding::new(cx, on.into_reactive_value(), move |on| {
        dom.set_custom_state(el, name, *on);
    })
}

/// Binds one imperative property, which is neither an attribute nor visible to selectors.
pub fn bind_property<M>(
    cx: &BuildCx<'_>,
    el: NodeId,
    key: PropKey,
    value: impl IntoReactiveValue<PropValue, M>,
) -> Binding {
    let dom = cx.dom_handle();
    Binding::new(cx, value.into_reactive_value(), move |value| {
        dom.set_property(el, key, value.clone());
    })
}

/// Binds what this element means to an accessibility tree.
///
/// The accumulator is lowered to a resolved [`Semantics`] inside the binding's own effect, so a
/// property that reads a signal re-runs this and nothing else.
pub fn bind_semantics(cx: &BuildCx<'_>, el: NodeId, a11y: A11yBinding) -> Binding {
    let dom = cx.dom_handle();
    let value = crate::value::ReactiveValue::derive(move || a11y.lower());
    Binding::new(cx, value, move |semantics: &Semantics| {
        dom.set_semantics(el, Some(semantics));
    })
}

#[cfg(test)]
mod tests {
    use zgui_interned::{AttrName, ClassName, ElementName};
    use zgui_reactive::prelude::*;
    use zgui_reactive::{RwSignal, flush};
    use zgui_vocab::{Role, SemanticFlags, UiState};

    use super::{bind_attribute, bind_class, bind_custom_state, bind_semantics, bind_ui_state};
    use crate::binding::a11y::A11yBinding;
    use crate::fixture::Fixture;

    #[test]
    fn a_reactive_attribute_follows_its_signal_at_the_flush() {
        let f = Fixture::new();
        let el = f.dom.create_element(ElementName::new("box"));
        let open = f.window.with(|| RwSignal::new(false));

        let _binding = f.window.with(|| {
            bind_attribute(&f.cx(), el, AttrName::new("data-state"), move || {
                Some(if open.get() { "open" } else { "closed" }.to_owned())
            })
        });
        assert_eq!(
            f.backend
                .attribute(el, AttrName::new("data-state"))
                .as_deref(),
            Some("closed")
        );

        open.set(true);
        flush();
        assert_eq!(
            f.backend
                .attribute(el, AttrName::new("data-state"))
                .as_deref(),
            Some("open")
        );
        f.window.unmount();
    }

    #[test]
    fn a_class_toggle_leaves_the_rest_of_the_list_alone() {
        let f = Fixture::new();
        let el = f.dom.create_element(ElementName::new("box"));
        f.dom.set_classes(el, &[ClassName::new("button")]);
        let busy = f.window.with(|| RwSignal::new(false));

        let _binding = f
            .window
            .with(|| bind_class(&f.cx(), el, ClassName::new("busy"), busy));
        assert_eq!(f.backend.classes(el), vec![ClassName::new("button")]);

        busy.set(true);
        flush();
        assert_eq!(
            f.backend.classes(el),
            vec![ClassName::new("button"), ClassName::new("busy")]
        );
        f.window.unmount();
    }

    #[test]
    fn a_state_binding_writes_a_state_a_view_is_allowed_to_assert() {
        let f = Fixture::new();
        let el = f.dom.create_element(ElementName::new("control"));
        let disabled = f.window.with(|| RwSignal::new(true));

        let _binding = f
            .window
            .with(|| bind_ui_state(&f.cx(), el, UiState::DISABLED, disabled));
        assert!(f.backend.ui_state(el).contains(UiState::DISABLED));

        disabled.set(false);
        flush();
        assert!(!f.backend.ui_state(el).contains(UiState::DISABLED));
        f.window.unmount();
    }

    #[test]
    fn a_custom_state_binding_writes_a_state_the_author_named() {
        let f = Fixture::new();
        let el = f.dom.create_element(ElementName::new("box"));
        let selected = f.window.with(|| RwSignal::new(false));
        let name = zgui_interned::Ident::new("selected");

        let _binding = f
            .window
            .with(|| bind_custom_state(&f.cx(), el, name, selected));
        assert!(!f.backend.has_custom_state(el, name));

        selected.set(true);
        flush();
        assert!(f.backend.has_custom_state(el, name));

        selected.set(false);
        flush();
        assert!(!f.backend.has_custom_state(el, name));
        f.window.unmount();
    }

    #[test]
    fn semantics_are_lowered_at_write_time_and_re_lowered_when_a_property_changes() {
        let f = Fixture::new();
        let el = f.dom.create_element(ElementName::new("control"));
        let disabled = f.window.with(|| RwSignal::new(false));

        let _binding = f.window.with(|| {
            bind_semantics(
                &f.cx(),
                el,
                A11yBinding::new(Role::Button)
                    .label("Save")
                    .disabled(disabled),
            )
        });
        let semantics = f.backend.semantics(el).expect("written");
        assert_eq!(semantics.role, Role::Button);
        assert!(!semantics.flags.contains(SemanticFlags::DISABLED));

        disabled.set(true);
        flush();
        assert!(
            f.backend
                .semantics(el)
                .expect("written")
                .flags
                .contains(SemanticFlags::DISABLED)
        );
        f.window.unmount();
    }
}
