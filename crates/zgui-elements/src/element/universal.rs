//! The attributes every element takes.
//!
//! Everything here is available on every name in the vocabulary, because every one of them is a
//! styled, selectable, listenable box. What is *not* here is the point of the split: `src` belongs
//! to a picture and `paths` to a drawing, and putting either on a row would be an attribute nothing
//! reads and no diagnostic anywhere.

use zgui_interned::{AttrName, ClassName, CustomPropertyName, Ident};
use zgui_view::{
    A11yBinding, Attrs, Classes, EventCx, EventType, IntoReactiveValue, ListenerOptions, NodeRef,
    PropKey, PropValue, ReactiveValue, UiState, erase,
};

use crate::element::Element;
use crate::element::attribute::{self, Attribute};
use crate::focus::Focus;
use crate::tag::Tag;

impl<T: Tag> Element<T> {
    /// Adds to the element's class list.
    ///
    /// Classes merge rather than replace, in the order they were added and without duplicates, so
    /// a component's own classes and a caller's classes end up in one list with the caller's last.
    #[must_use]
    pub fn class(mut self, classes: impl Into<Classes>) -> Self {
        self.classes = self.classes.merged(classes);
        self
    }

    /// Turns one class on or off, leaving the rest of the list alone.
    #[must_use]
    pub fn class_toggle<M>(self, class: ClassName, on: impl IntoReactiveValue<bool, M>) -> Self {
        self.push(Attribute::ClassToggle(class, on.into_reactive_value()))
    }

    /// Replaces the element's whole inline style text.
    ///
    /// This is the `style="…"` an author writes. It replaces every declaration the element carries
    /// of its own, including any [`Element::style_property`] set before it — which is why the two
    /// are not usually mixed on one element.
    #[must_use]
    pub fn style_text<M>(self, css: impl IntoReactiveValue<Option<String>, M>) -> Self {
        self.push(Attribute::StyleText(css.into_reactive_value()))
    }

    /// Sets or removes one inline style declaration, leaving the others alone.
    ///
    /// An unknown property, or a value that does not parse for it, is dropped with a diagnostic —
    /// the same treatment it would get in a style sheet.
    #[must_use]
    pub fn style_property<M>(
        self,
        property: impl Into<String>,
        value: impl IntoReactiveValue<Option<String>, M>,
    ) -> Self {
        self.push(Attribute::StyleProperty(
            property.into(),
            value.into_reactive_value(),
        ))
    }

    /// Sets or removes one custom property on the element.
    #[must_use]
    pub fn custom_property<M>(
        self,
        property: CustomPropertyName,
        value: impl IntoReactiveValue<Option<String>, M>,
    ) -> Self {
        self.push(Attribute::CustomProperty(
            property,
            value.into_reactive_value(),
        ))
    }

    /// Sets or removes one attribute, which selectors can see.
    #[must_use]
    pub fn attribute<M>(
        self,
        name: AttrName,
        value: impl IntoReactiveValue<Option<String>, M>,
    ) -> Self {
        self.push(Attribute::Attribute(name, value.into_reactive_value()))
    }

    /// Turns one interaction state on or off.
    ///
    /// Only the states a view is allowed to assert about its own element — the control states that
    /// mirror an accessibility property. Hover, focus and activation are computed by the input
    /// system, and a view that could assert one would be lying to it.
    #[must_use]
    pub fn state<M>(self, state: UiState, on: impl IntoReactiveValue<bool, M>) -> Self {
        self.push(Attribute::State(state, on.into_reactive_value()))
    }

    /// Turns one author-defined state on or off, matched by `:state(name)`.
    #[must_use]
    pub fn custom_state<M>(self, name: Ident, on: impl IntoReactiveValue<bool, M>) -> Self {
        self.push(Attribute::CustomState(name, on.into_reactive_value()))
    }

    /// Sets one imperative property, which is neither an attribute nor visible to selectors.
    #[must_use]
    pub fn property<M>(self, key: PropKey, value: impl IntoReactiveValue<PropValue, M>) -> Self {
        self.push(Attribute::Property(key, value.into_reactive_value()))
    }

    /// Listens for `event` on this element.
    ///
    /// The handler's argument type is inferred from the event, so a listener for a pointer event
    /// reads pointer fields and a listener for a key event reads key fields, with no annotation and
    /// no downcast.
    ///
    /// # Naming a handler before attaching it
    ///
    /// A closure written here needs no annotation, because the event is already known when the
    /// compiler reads it. A closure bound to a name *first* is read before anything has said what
    /// it will be used for, so its argument type is settled without that knowledge and it is then
    /// rejected here — as `implementation of Fn is not general enough`, which says nothing about
    /// what to do. Build it with [`handler`](zgui_view::handler), which supplies the event at the
    /// binding:
    ///
    /// ```
    /// use zgui_elements::control;
    /// use zgui_view::{events, handler};
    ///
    /// let pick = handler(events::CLICK, move |_| { /* … */ });
    /// let _ = control().on(events::CLICK, pick);
    /// ```
    #[must_use]
    pub fn on<E: EventType>(
        self,
        event: E,
        handler: impl Fn(&mut EventCx<'_, E>) + 'static,
    ) -> Self {
        self.on_with(event, ListenerOptions::DEFAULT, handler)
    }

    /// Listens for `event`, saying how.
    ///
    /// What [`Element::on`] does, plus the choice of which leg of the dispatch the handler runs in
    /// and whether it runs more than once.
    #[must_use]
    pub fn on_with<E: EventType>(
        self,
        event: E,
        options: ListenerOptions,
        handler: impl Fn(&mut EventCx<'_, E>) + 'static,
    ) -> Self {
        let kind = event.kind();
        self.push(Attribute::Listener(kind, options, erase(event, handler)))
    }

    /// Describes what this element means, for anything reading the interface rather than looking
    /// at it.
    ///
    /// Merges with whatever was described already, with the later description winning where the
    /// two disagree — so a caller's `a11y:` on a component call beats the component's own, which is
    /// right, because the caller knows the context the control sits in.
    #[must_use]
    pub fn a11y(mut self, a11y: A11yBinding) -> Self {
        self.a11y = Some(match self.a11y {
            Some(existing) => existing.merged(&a11y),
            None => a11y,
        });
        self
    }

    /// Fills `handle` in with this element once it exists.
    #[must_use]
    pub fn node_ref(self, handle: NodeRef) -> Self {
        self.push(Attribute::NodeRef(handle))
    }

    /// Makes the element focusable, and says how it is reached.
    ///
    /// Typed rather than an attribute spelled by hand, because `tabindx="0"` is a control nobody
    /// can reach and nothing anywhere reports it.
    ///
    /// Takes a value like any other attribute, so how an element is reached can follow a signal.
    /// That is not a refinement: a control that is disabled while it holds focus has to leave the
    /// sequential order, and a composite control moves the one sequentially reachable item between
    /// its children as the arrow keys move through them.
    ///
    /// ```
    /// use zgui_elements::{Focus, control};
    /// use zgui_reactive::prelude::*;
    /// use zgui_reactive::{Mounted, RwSignal, install};
    ///
    /// install().unwrap();
    /// let window = Mounted::new();
    /// let disabled = window.with(|| RwSignal::new(false));
    ///
    /// let button = control().tabindex(move || {
    ///     if disabled.get() {
    ///         Focus::Programmatic
    ///     } else {
    ///         Focus::Sequential
    ///     }
    /// });
    /// window.unmount();
    /// ```
    #[must_use]
    pub fn tabindex<M>(self, focus: impl IntoReactiveValue<Focus, M>) -> Self {
        match focus.into_reactive_value() {
            ReactiveValue::Constant(focus) => {
                self.attribute(AttrName::new("tabindex"), focus.as_str())
            }
            focus => self.attribute(AttrName::new("tabindex"), move || {
                Some(focus.get().as_str().to_owned())
            }),
        }
    }

    /// Hides the element from layout and from everything reading the interface.
    #[must_use]
    pub fn hidden<M>(self, hidden: impl IntoReactiveValue<bool, M>) -> Self {
        let hidden = hidden.into_reactive_value();
        self.attribute(AttrName::new("hidden"), move || {
            hidden.get().then(String::new)
        })
    }

    /// Puts a bundle a caller forwarded onto this element.
    ///
    /// The bundle's classes merge after the element's own, its accessibility description merges
    /// over the element's own, and its toggles, attributes and listeners are applied after the
    /// ones already added — which is the whole of what makes `<Button class="w-full"/>` and
    /// `<Button on:click=…/>` work on a component that never mentioned either.
    #[must_use]
    pub fn attrs(mut self, attrs: Attrs) -> Self {
        let (entries, a11y, classes) = attrs.into_parts();
        attribute::replay(entries, classes, &mut self.attributes, &mut self.classes);
        match a11y {
            Some(a11y) => self.a11y(a11y),
            None => self,
        }
    }
}
