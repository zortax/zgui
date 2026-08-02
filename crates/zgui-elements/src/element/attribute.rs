//! One thing an element was told, and how it is written.

use std::rc::Rc;

use zgui_interned::{AttrName, ClassName, CustomPropertyName, Ident};
use zgui_view::binding::{
    bind_attribute, bind_class, bind_custom_property, bind_custom_state, bind_property,
    bind_style_property, bind_ui_state,
};
use zgui_view::{
    AttrEntry, Binding, BuildCx, Classes, EventCx, EventKind, ListenerOptions,
    ListenerRegistration, NodeId, NodeRef, PropKey, PropValue, ReactiveValue, UiState,
};

/// One thing an element was told to be, in the order it was written.
///
/// A whole class list and an accessibility description are not here: those two *accumulate* rather
/// than happening in sequence, because a component's classes and a caller's classes have to merge
/// into one list rather than overwrite one another. Everything else applies in order, which is what
/// makes two toggles of the same class resolve caller-last.
#[allow(
    clippy::enum_variant_names,
    reason = "three of the ten really are an attribute, a state and a property of the same shape; \
               renaming them to satisfy the lint would rename them away from what they are"
)]
pub(crate) enum Attribute {
    /// One class, toggled.
    ClassToggle(ClassName, ReactiveValue<bool>),
    /// The whole inline style text.
    StyleText(ReactiveValue<Option<String>>),
    /// One inline style declaration.
    StyleProperty(String, ReactiveValue<Option<String>>),
    /// One custom property.
    CustomProperty(CustomPropertyName, ReactiveValue<Option<String>>),
    /// One attribute, which selectors can see.
    Attribute(AttrName, ReactiveValue<Option<String>>),
    /// One interaction state a view is allowed to assert.
    State(UiState, ReactiveValue<bool>),
    /// One author-defined state, matched by `:state(name)`.
    CustomState(Ident, ReactiveValue<bool>),
    /// One imperative property, which selectors cannot see.
    Property(PropKey, ReactiveValue<PropValue>),
    /// One listener.
    Listener(EventKind, ListenerOptions, Rc<dyn Fn(&mut EventCx<'_>)>),
    /// A handle to fill in with the element once it exists.
    NodeRef(NodeRef),
}

impl Attribute {
    /// Applies this to `el`, returning whatever has to be kept alive for it to keep applying.
    ///
    /// A listener's registration goes into `listeners` rather than coming back as a binding: it is
    /// not kept alive by being held, it is *removed* by being held, and the element has to take
    /// its previous listeners off before it registers this description's.
    pub(crate) fn apply(
        self,
        cx: &BuildCx<'_>,
        el: NodeId,
        listeners: &mut Vec<ListenerRegistration>,
    ) -> Option<Binding> {
        match self {
            Self::ClassToggle(class, on) => Some(bind_class(cx, el, class, on)),
            Self::StyleText(css) => {
                let dom = cx.dom_handle();
                Some(Binding::new(cx, css, move |css: &Option<String>| {
                    dom.set_style_text(el, css.as_deref());
                }))
            }
            Self::StyleProperty(property, value) => {
                Some(bind_style_property(cx, el, property, value))
            }
            Self::CustomProperty(property, value) => {
                Some(bind_custom_property(cx, el, property, value))
            }
            Self::Attribute(name, value) => Some(bind_attribute(cx, el, name, value)),
            Self::State(state, on) => Some(bind_ui_state(cx, el, state, on)),
            Self::CustomState(name, on) => Some(bind_custom_state(cx, el, name, on)),
            Self::Property(key, value) => Some(bind_property(cx, el, key, value)),
            Self::Listener(event, options, handler) => {
                listeners.push(ListenerRegistration::erased(
                    &**cx.dom(),
                    el,
                    event,
                    options,
                    handler,
                ));
                None
            }
            Self::NodeRef(handle) => {
                handle.bind(el, cx.dom(), cx.host());
                None
            }
        }
    }
}

/// Replays a bundle a caller forwarded, appending its entries after whatever is already there.
///
/// This is the consumer side of `{..attrs}`: a component that takes a bundle and spreads it onto
/// its root element gets the caller's classes merged after its own, the caller's accessibility
/// description merged over its own, and the caller's toggles, attributes and listeners applied
/// after its own — which is what makes `<Button class="w-full" on:click=…/>` mean what it reads as.
pub(crate) fn replay(
    entries: Vec<AttrEntry>,
    forwarded: Classes,
    into: &mut Vec<Attribute>,
    classes: &mut Classes,
) {
    *classes = classes.merged(&forwarded);
    for entry in entries {
        into.push(match entry {
            AttrEntry::ClassToggle(class, on) => Attribute::ClassToggle(class, on),
            AttrEntry::StyleProperty(property, value) => Attribute::StyleProperty(property, value),
            AttrEntry::CustomProperty(property, value) => {
                Attribute::CustomProperty(property, value)
            }
            AttrEntry::Attribute(name, value) => Attribute::Attribute(name, value),
            AttrEntry::StateToggle(state, on) => Attribute::State(state, on),
            AttrEntry::CustomStateToggle(name, on) => Attribute::CustomState(name, on),
            AttrEntry::Property(key, value) => Attribute::Property(key, value),
            AttrEntry::Listener(event, options, handler) => {
                Attribute::Listener(event, options, handler)
            }
        });
    }
}
