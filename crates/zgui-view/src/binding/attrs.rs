//! A bundle of attributes forwarded from a caller to a component's root element.

use std::rc::Rc;

use zgui_interned::{AttrName, ClassName, CustomPropertyName, Ident};
use zgui_vocab::{EventKind, ListenerOptions, PropKey, PropValue, UiState};

use crate::binding::a11y::A11yBinding;
use crate::binding::classes::Classes;
use crate::event::EventCx;
use crate::value::{IntoReactiveValue, ReactiveValue};

/// One thing a caller asked to be put on a component's root element.
///
/// Cloning one is a reference-count bump: every payload a caller can write is either a plain
/// value or already behind an `Rc`.
#[derive(Clone)]
pub enum AttrEntry {
    /// One class, toggled.
    ClassToggle(ClassName, ReactiveValue<bool>),
    /// One inline style declaration.
    StyleProperty(String, ReactiveValue<Option<String>>),
    /// One custom property.
    CustomProperty(CustomPropertyName, ReactiveValue<Option<String>>),
    /// One attribute.
    Attribute(AttrName, ReactiveValue<Option<String>>),
    /// One interaction state a view may assert.
    StateToggle(UiState, ReactiveValue<bool>),
    /// One author-defined state, matched by `:state(name)`.
    CustomStateToggle(Ident, ReactiveValue<bool>),
    /// One imperative property.
    Property(PropKey, ReactiveValue<PropValue>),
    /// One listener.
    Listener(EventKind, ListenerOptions, Rc<dyn Fn(&mut EventCx<'_>)>),
}

/// Attributes, classes, styles, listeners and accessibility properties forwarded from a caller.
///
/// Without this, a caller cannot write `class:mine=true`, `on:pointer_down=…` or
/// `attr:data-testid="x"` on a component and have any of the three land on the element that
/// component actually renders — and every composition in a component library grows a wrapper
/// element it did not need.
///
/// The merge rules are the ones the whole styling contract rests on:
///
/// * a whole-list `class` on a component call goes to the component's `class` **property**, never
///   into this bundle, and the component merges it after its own variant classes — which is what
///   makes "the caller's class wins" a contract rather than an ordering accident;
/// * class and style toggles **compose** with the component's own, applied afterwards, and two
///   toggles of the same name resolve caller-last;
/// * accessibility properties from the caller **win**, because the caller knows the context;
/// * listeners **accumulate**: a caller's `on:click` does not replace the component's, and both
///   run in registration order, the component's first;
/// * attributes, custom properties and imperative properties are last-write-wins, caller-last —
///   which is what lets a caller size or colour a component the component's own sheet reads from,
///   `var:--zui-table-columns` on the call standing in for a declaration it has nowhere to write.
///
/// ```
/// use zgui_interned::ClassName;
/// use zgui_view::Attrs;
///
/// let attrs = Attrs::new().class_toggle(ClassName::new("mine"), true);
/// assert_eq!(attrs.len(), 1);
///
/// // A bundle a component has to put on an element it rebuilds more than once — the content of a
/// // `Presence`, a branch of a `Show` — is cloned rather than consumed.
/// let again = attrs.clone();
/// assert_eq!(again.len(), 1);
/// ```
#[derive(Clone, Default)]
pub struct Attrs {
    /// What the caller asked for, in the order it was written.
    entries: Vec<AttrEntry>,
    /// The accessibility properties the caller set.
    a11y: Option<A11yBinding>,
    /// A whole class list the caller passed as a component property.
    classes: Classes,
}

impl Attrs {
    /// An empty bundle.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many entries the bundle holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the bundle holds nothing at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.a11y.is_none() && self.classes.is_empty()
    }

    /// The entries, in the order they were written.
    pub fn entries(&self) -> &[AttrEntry] {
        &self.entries
    }

    /// The accessibility properties the caller set.
    pub fn a11y(&self) -> Option<&A11yBinding> {
        self.a11y.as_ref()
    }

    /// The class list the caller passed.
    pub fn classes(&self) -> &Classes {
        &self.classes
    }

    /// Takes the bundle apart, for whoever is going to put it onto an element.
    ///
    /// The three parts are separate because they are applied differently: the entries happen in
    /// order, and the class list and the accessibility description merge with whatever the element
    /// already had.
    pub fn into_parts(self) -> (Vec<AttrEntry>, Option<A11yBinding>, Classes) {
        (self.entries, self.a11y, self.classes)
    }

    /// Adds a whole class list.
    #[must_use]
    pub fn classes_from(mut self, classes: Classes) -> Self {
        self.classes = self.classes.merged(&classes);
        self
    }

    /// Adds one toggled class.
    #[must_use]
    pub fn class_toggle<M>(
        mut self,
        class: ClassName,
        on: impl IntoReactiveValue<bool, M>,
    ) -> Self {
        self.entries
            .push(AttrEntry::ClassToggle(class, on.into_reactive_value()));
        self
    }

    /// Adds one inline style declaration.
    #[must_use]
    pub fn style_property<M>(
        mut self,
        property: impl Into<String>,
        value: impl IntoReactiveValue<Option<String>, M>,
    ) -> Self {
        self.entries.push(AttrEntry::StyleProperty(
            property.into(),
            value.into_reactive_value(),
        ));
        self
    }

    /// Adds one custom property.
    #[must_use]
    pub fn custom_property<M>(
        mut self,
        property: CustomPropertyName,
        value: impl IntoReactiveValue<Option<String>, M>,
    ) -> Self {
        self.entries.push(AttrEntry::CustomProperty(
            property,
            value.into_reactive_value(),
        ));
        self
    }

    /// Adds one attribute.
    #[must_use]
    pub fn attribute<M>(
        mut self,
        name: AttrName,
        value: impl IntoReactiveValue<Option<String>, M>,
    ) -> Self {
        self.entries
            .push(AttrEntry::Attribute(name, value.into_reactive_value()));
        self
    }

    /// Adds one interaction state, toggled.
    #[must_use]
    pub fn state<M>(mut self, state: UiState, on: impl IntoReactiveValue<bool, M>) -> Self {
        self.entries
            .push(AttrEntry::StateToggle(state, on.into_reactive_value()));
        self
    }

    /// Adds one author-defined state, toggled.
    #[must_use]
    pub fn custom_state<M>(mut self, name: Ident, on: impl IntoReactiveValue<bool, M>) -> Self {
        self.entries
            .push(AttrEntry::CustomStateToggle(name, on.into_reactive_value()));
        self
    }

    /// Adds one imperative property.
    #[must_use]
    pub fn property<M>(
        mut self,
        key: PropKey,
        value: impl IntoReactiveValue<PropValue, M>,
    ) -> Self {
        self.entries
            .push(AttrEntry::Property(key, value.into_reactive_value()));
        self
    }

    /// Adds one listener.
    #[must_use]
    pub fn listener<E: crate::event::EventType>(
        mut self,
        event: E,
        options: ListenerOptions,
        handler: impl Fn(&mut EventCx<'_, E>) + 'static,
    ) -> Self {
        self.entries.push(AttrEntry::Listener(
            event.kind(),
            options,
            crate::event::erase(event, handler),
        ));
        self
    }

    /// Sets the accessibility properties, merging over any that were already there.
    #[must_use]
    pub fn a11y_from(mut self, a11y: A11yBinding) -> Self {
        self.a11y = Some(match self.a11y {
            Some(existing) => existing.merged(&a11y),
            None => a11y,
        });
        self
    }

    /// Appends `other`'s entries after this bundle's, so `other` wins where the two disagree.
    #[must_use]
    pub fn merged(mut self, other: Self) -> Self {
        self.classes = self.classes.merged(&other.classes);
        self.entries.extend(other.entries);
        self.a11y = match (self.a11y, other.a11y) {
            (Some(mine), Some(theirs)) => Some(mine.merged(&theirs)),
            (mine, theirs) => theirs.or(mine),
        };
        self
    }
}

#[cfg(test)]
mod tests {
    use zgui_interned::{AttrName, ClassName};
    use zgui_vocab::Role;

    use super::{AttrEntry, Attrs};
    use crate::binding::a11y::A11yBinding;
    use crate::binding::classes::Classes;

    #[test]
    fn entries_keep_the_order_they_were_written_in_so_the_caller_is_last() {
        let component = Attrs::new().attribute(AttrName::new("data-x"), Some("one".to_owned()));
        let caller = Attrs::new().attribute(AttrName::new("data-x"), Some("two".to_owned()));
        let merged = component.merged(caller);

        let values: Vec<String> = merged
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                AttrEntry::Attribute(_, value) => value.get(),
                _ => None,
            })
            .collect();
        assert_eq!(values, vec!["one".to_owned(), "two".to_owned()]);
    }

    #[test]
    fn listeners_accumulate_rather_than_replacing_one_another() {
        let merged = Attrs::new()
            .listener(crate::events::CLICK, Default::default(), |_| {})
            .merged(Attrs::new().listener(crate::events::CLICK, Default::default(), |_| {}));
        let listeners = merged
            .entries()
            .iter()
            .filter(|entry| matches!(entry, AttrEntry::Listener(..)))
            .count();
        assert_eq!(listeners, 2);
    }

    #[test]
    fn a_callers_accessibility_properties_win() {
        let merged = Attrs::new()
            .a11y_from(A11yBinding::new(Role::Button).label("component"))
            .merged(Attrs::new().a11y_from(A11yBinding::new(Role::Button).label("caller")));
        assert_eq!(
            merged.a11y().expect("set").lower().label.as_deref(),
            Some("caller")
        );
    }

    #[test]
    fn a_whole_class_list_is_merged_rather_than_replaced() {
        let merged = Attrs::new()
            .classes_from(Classes::from("button"))
            .merged(Attrs::new().classes_from(Classes::from("w-full")));
        assert_eq!(
            merged.classes().names(),
            [ClassName::new("button"), ClassName::new("w-full")]
        );
    }
}
