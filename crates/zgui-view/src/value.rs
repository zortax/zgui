//! Values that may or may not change.
//!
//! Whether an attribute is static or dynamic is decided by its *type*, not by how it was written.
//! A literal is written to the backend once, at build time, with no effect behind it; a signal or
//! a closure gets exactly one effect. An author never annotates the difference, and that is the
//! whole point of this module.

use std::rc::Rc;

use zgui_reactive::prelude::*;

/// Which shape a value arrived in.
///
/// The marker is inferred at every call site and never written by hand. It exists so that one
/// method can accept a constant, a signal and a closure without three names.
#[doc(hidden)]
pub mod marker {
    /// A value that never changes.
    pub struct Constant;
    /// A closure computing a value.
    pub struct Derived;
    /// A signal, a memo or anything else readable.
    pub struct Reactive;
    /// A value written where an optional one was wanted, which is therefore present.
    pub struct Present;
    /// A value that has already been through this conversion.
    pub struct Already;
    /// A handle on an element, written where the element itself was wanted.
    pub struct Related;
    /// A closure choosing which element a relation names, or none.
    pub struct RelatedChoice;
    /// A closure computing an `R` that converts into the value wanted.
    pub struct DerivedInto<R>(core::marker::PhantomData<R>);
    /// A value given where an optional one was wanted.
    pub struct Given;
    /// An `Option` given where an optional value was wanted.
    pub struct Maybe;
}

/// The values written where an optional `T` is wanted that stand for a present one.
///
/// Sealed, and listed one by one rather than for everything that converts into `T`: a blanket
/// list would also hold `T` itself, which the constant conversion already takes through
/// `T: Into<Option<T>>`, and an owned `String` written where an `Option<String>` is wanted would
/// then have two meanings.
pub trait PresentAs<T>: sealed::Present<T> {
    /// The present value.
    fn present(self) -> T;
}

/// What closes [`PresentAs`] to the impls here.
mod sealed {
    /// A source [`PresentAs`](super::PresentAs) is implemented for.
    pub trait Present<T> {}
}

/// Implements [`PresentAs`] for sources that convert with `Into`.
macro_rules! present_as {
    ($target:ty: $($source:ty),+ $(,)?) => {
        $(
            impl sealed::Present<$target> for $source {}
            impl PresentAs<$target> for $source {
                fn present(self) -> $target {
                    self.into()
                }
            }
        )+
    };
}

present_as!(String: &str, &mut str, Box<str>, std::borrow::Cow<'_, str>, char);
present_as!(zgui_vocab::SharedString: &str, String);

impl sealed::Present<String> for zgui_vocab::SharedString {}
impl PresentAs<String> for zgui_vocab::SharedString {
    fn present(self) -> String {
        self.as_str().to_owned()
    }
}

impl<T: Clone> sealed::Present<T> for &T {}
impl<T: Clone> PresentAs<T> for &T {
    fn present(self) -> T {
        self.clone()
    }
}

/// A value a binding writes: either fixed, or computed each time the binding runs.
pub enum ReactiveValue<T> {
    /// A value that never changes.
    Constant(T),
    /// A value computed on each run of the binding that holds it.
    Dynamic(Rc<dyn Fn() -> T>),
}

impl<T: Clone> ReactiveValue<T> {
    /// The value now.
    ///
    /// Reading a dynamic value inside an effect subscribes that effect to whatever the closure
    /// read; reading a constant subscribes to nothing.
    pub fn get(&self) -> T {
        match self {
            Self::Constant(value) => value.clone(),
            Self::Dynamic(compute) => compute(),
        }
    }
}

impl<T> ReactiveValue<T> {
    /// Whether this value can ever change.
    ///
    /// A binding over a constant runs once and needs no effect at all, which is the optimisation
    /// this answers.
    pub fn is_constant(&self) -> bool {
        matches!(self, Self::Constant(_))
    }

    /// Builds a dynamic value from a closure.
    pub fn derive(compute: impl Fn() -> T + 'static) -> Self {
        Self::Dynamic(Rc::new(compute))
    }
}

impl<T> Clone for ReactiveValue<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        match self {
            Self::Constant(value) => Self::Constant(value.clone()),
            Self::Dynamic(compute) => Self::Dynamic(Rc::clone(compute)),
        }
    }
}

/// Anything that can be written where a `T` is wanted.
///
/// Implemented for `T` itself and anything that converts into it, for any closure returning one,
/// and for every readable reactive value. `M` is the marker that tells those three apart and is
/// always inferred.
///
/// A binding runs a closure again under the view's owner, with no cleanup between runs. A memo,
/// signal, context or cleanup that the closure creates is kept until the view is unmounted, once
/// for each run. Create those outside the closure.
///
/// ```
/// use zgui_reactive::{Mounted, RwSignal, install};
/// use zgui_view::{IntoReactiveValue, ReactiveValue};
///
/// install().unwrap();
/// let node = Mounted::new();
///
/// fn width<M>(value: impl IntoReactiveValue<String, M>) -> ReactiveValue<String> {
///     value.into_reactive_value()
/// }
///
/// // A literal is constant; a closure and a signal are not.
/// assert!(width("12px").is_constant());
/// assert!(!width(|| "12px".to_owned()).is_constant());
///
/// let size = node.with(|| RwSignal::new("12px".to_owned()));
/// assert!(!width(size).is_constant());
/// assert_eq!(width(size).get(), "12px");
/// node.unmount();
/// ```
pub trait IntoReactiveValue<T, M> {
    /// Converts.
    fn into_reactive_value(self) -> ReactiveValue<T>;
}

impl<T, U: Into<T>> IntoReactiveValue<T, marker::Constant> for U {
    fn into_reactive_value(self) -> ReactiveValue<T> {
        ReactiveValue::Constant(self.into())
    }
}

/// Anything that can be written where an optional `T` is wanted.
///
/// Implemented for anything that converts into `T`, which is present, and for an `Option` of
/// anything that converts into `T`, which can be absent. The second form lets a component that
/// wraps another one pass on its own optional prop. `M` is the marker that tells the two apart and
/// is always inferred.
///
/// ```
/// use zgui_view::IntoOptionalProp;
///
/// fn label<M>(value: impl IntoOptionalProp<String, M>) -> Option<String> {
///     value.into_optional_prop()
/// }
///
/// assert_eq!(label("Name"), Some("Name".to_owned()));
/// assert_eq!(label(Some("Name")), Some("Name".to_owned()));
/// assert_eq!(label(None::<String>), None);
/// ```
pub trait IntoOptionalProp<T, M> {
    /// Converts.
    fn into_optional_prop(self) -> Option<T>;
}

impl<T, U: Into<T>> IntoOptionalProp<T, marker::Given> for U {
    fn into_optional_prop(self) -> Option<T> {
        Some(self.into())
    }
}

impl<T, U: Into<T>> IntoOptionalProp<T, marker::Maybe> for Option<U> {
    fn into_optional_prop(self) -> Option<T> {
        self.map(Into::into)
    }
}

/// A present value, written where an optional one was wanted.
///
/// `attr:data-part="root"` and `style:gap="1rem"` are what an author writes, and an attribute or a
/// declaration is optional because writing `None` is how one is removed. This is what closes the
/// gap between the two, so that saying nothing about absence means presence.
///
/// The value itself, `Some(value)` and anything else that converts into `Option<T>` take the
/// constant conversion; this one takes what [`PresentAs`] lists, such as a `&str` written where an
/// `Option<String>` is wanted.
impl<T, U: PresentAs<T>> IntoReactiveValue<Option<T>, marker::Present> for U {
    fn into_reactive_value(self) -> ReactiveValue<Option<T>> {
        ReactiveValue::Constant(Some(self.present()))
    }
}

/// A value that is already one.
///
/// What lets a bundle of attributes a caller forwarded be replayed onto an element through the
/// same methods an author writes by hand: the values in it have already been converted once, and
/// converting them again would mean a second set of methods that do the same thing.
impl<T> IntoReactiveValue<T, marker::Already> for ReactiveValue<T> {
    fn into_reactive_value(self) -> ReactiveValue<T> {
        self
    }
}

/// A handle on an element, written where an accessibility relation wanted the element.
///
/// Every relation — `labelled_by`, `described_by`, `controls`, `owns` — names a node, and what a
/// view has is the [`NodeRef`](crate::NodeRef) it wrote on that node. Without this, relating one
/// element to another means reaching for the identifier arithmetic that turns one name for a node
/// into the other's, in every component that has a label.
///
/// It tracks, so a relation written before its target exists is filled in on the frame the target
/// mounts, and emptied again on the frame it goes away — which is what stops a control from
/// naming a node the tree no longer has.
///
/// ```
/// use zgui_reactive::{Mounted, install};
/// use zgui_view::{A11yBinding, NodeRef};
/// use zgui_vocab::Role;
///
/// install().ok();
/// let window = Mounted::new();
/// let label = window.with(NodeRef::new);
///
/// // Unbound, the relation names nothing an accessibility tree can resolve.
/// let field = A11yBinding::new(Role::TextInput).labelled_by(label);
/// assert_eq!(field.lower().relations.labelled_by, [zgui_vocab::NodeId(0)]);
/// window.unmount();
/// ```
impl IntoReactiveValue<zgui_vocab::NodeId, marker::Related> for crate::NodeRef {
    fn into_reactive_value(self) -> ReactiveValue<zgui_vocab::NodeId> {
        ReactiveValue::Dynamic(Rc::new(move || {
            zgui_vocab::NodeId(self.get().map_or(0, crate::NodeId::as_u64))
        }))
    }
}

/// A closure choosing *which* element a relation names, or none at all.
///
/// The other half of the story [`marker::Related`] starts. A relation to one fixed element is a
/// handle; a relation whose target moves is not — a field describes its hint until it is wrong and
/// its error message afterwards, and a menu's active descendant is a different item every time an
/// arrow key is pressed. Without this, a component in that position is back to turning one name
/// for a node into another by hand, which is the arithmetic the handle conversion exists to
/// remove.
///
/// `None` names nothing, and names it in a way an accessibility tree resolves to no relation at
/// all — the same answer an unbound handle gives.
///
/// ```
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{Mounted, RwSignal, install};
/// use zgui_view::{A11yBinding, NodeRef};
/// use zgui_vocab::Role;
///
/// install().ok();
/// let window = Mounted::new();
/// let (hint, error) = window.with(|| (NodeRef::new(), NodeRef::new()));
/// let wrong = window.with(|| RwSignal::new(false));
///
/// // Whichever of the two is the right thing to read out right now.
/// let field = A11yBinding::new(Role::TextInput)
///     .described_by(move || Some(if wrong.get() { error } else { hint }));
/// assert_eq!(field.lower().relations.described_by.len(), 1);
/// window.unmount();
/// ```
impl<F> IntoReactiveValue<zgui_vocab::NodeId, marker::RelatedChoice> for F
where
    F: Fn() -> Option<crate::NodeRef> + 'static,
{
    fn into_reactive_value(self) -> ReactiveValue<zgui_vocab::NodeId> {
        ReactiveValue::Dynamic(Rc::new(move || {
            zgui_vocab::NodeId(
                self()
                    .and_then(|node| node.get())
                    .map_or(0, crate::NodeId::as_u64),
            )
        }))
    }
}

impl<T, F: Fn() -> T + 'static> IntoReactiveValue<T, marker::Derived> for F {
    fn into_reactive_value(self) -> ReactiveValue<T> {
        ReactiveValue::Dynamic(Rc::new(self))
    }
}

/// A closure returning text, written where a shared string is wanted: `move || "Pause"`.
impl<F> IntoReactiveValue<zgui_vocab::SharedString, marker::DerivedInto<&'static str>> for F
where
    F: Fn() -> &'static str + 'static,
{
    fn into_reactive_value(self) -> ReactiveValue<zgui_vocab::SharedString> {
        ReactiveValue::Dynamic(Rc::new(move || self().into()))
    }
}

/// A closure returning an owned string, written where a shared string is wanted.
impl<F> IntoReactiveValue<zgui_vocab::SharedString, marker::DerivedInto<String>> for F
where
    F: Fn() -> String + 'static,
{
    fn into_reactive_value(self) -> ReactiveValue<zgui_vocab::SharedString> {
        ReactiveValue::Dynamic(Rc::new(move || self().into()))
    }
}

/// A closure returning static text, written where an owned string is wanted.
impl<F> IntoReactiveValue<String, marker::DerivedInto<&'static str>> for F
where
    F: Fn() -> &'static str + 'static,
{
    fn into_reactive_value(self) -> ReactiveValue<String> {
        ReactiveValue::Dynamic(Rc::new(move || self().to_owned()))
    }
}

impl<T, G> IntoReactiveValue<T, marker::Reactive> for G
where
    G: Get<Value = T> + Copy + 'static,
    T: Clone + 'static,
{
    fn into_reactive_value(self) -> ReactiveValue<T> {
        ReactiveValue::Dynamic(Rc::new(move || self.get()))
    }
}

#[cfg(test)]
mod tests {
    use zgui_reactive::prelude::*;
    use zgui_reactive::{Mounted, RwSignal, install};

    use super::{IntoReactiveValue, ReactiveValue};

    fn value<M>(source: impl IntoReactiveValue<i32, M>) -> ReactiveValue<i32> {
        source.into_reactive_value()
    }

    #[test]
    fn a_relation_can_change_which_element_it_names() {
        // A field describes its hint until the value is wrong and its error message afterwards. A
        // relation that could only be a fixed handle would leave that component doing identifier
        // arithmetic, which is what the handle conversion exists to remove.
        install().ok();
        let window = Mounted::new();
        let (hint, error) = window.with(|| (crate::NodeRef::new(), crate::NodeRef::new()));
        let wrong = window.with(|| RwSignal::new(false));

        let chosen: ReactiveValue<zgui_vocab::NodeId> =
            (move || Some(if wrong.get() { error } else { hint })).into_reactive_value();
        assert!(!chosen.is_constant());
        // Neither handle is bound, so both resolve to the same nothing — what is under test is
        // that the choice is re-made on every read rather than captured once.
        assert_eq!(chosen.get(), zgui_vocab::NodeId(0));
        wrong.set(true);
        assert_eq!(chosen.get(), zgui_vocab::NodeId(0));

        let none: ReactiveValue<zgui_vocab::NodeId> =
            (|| None::<crate::NodeRef>).into_reactive_value();
        assert_eq!(
            none.get(),
            zgui_vocab::NodeId(0),
            "no element is no relation"
        );
        window.unmount();
    }

    #[test]
    fn a_constant_is_constant_and_a_signal_is_not() {
        install().ok();
        let node = Mounted::new();
        let signal = node.with(|| RwSignal::new(1));

        assert!(value(5).is_constant());
        assert!(!value(signal).is_constant());
        assert!(!value(|| 7).is_constant());
        node.unmount();
    }

    #[test]
    fn a_dynamic_value_reports_what_its_source_says_now() {
        install().ok();
        let node = Mounted::new();
        let signal = node.with(|| RwSignal::new(1));
        let bound = value(signal);

        assert_eq!(bound.get(), 1);
        signal.set(9);
        assert_eq!(bound.get(), 9);
        node.unmount();
    }

    #[test]
    fn an_owned_string_is_present_where_an_optional_one_is_wanted() {
        fn optional<M>(source: impl IntoReactiveValue<Option<String>, M>) -> Option<String> {
            source.into_reactive_value().get()
        }
        assert_eq!(optional(String::from("a")), Some("a".to_owned()));
        assert_eq!(optional("b"), Some("b".to_owned()));
        assert_eq!(optional(&String::from("c")), Some("c".to_owned()));
        assert_eq!(optional(None::<String>), None);
    }

    #[test]
    fn a_closure_returning_text_is_a_shared_string() {
        fn shared<M>(
            source: impl IntoReactiveValue<zgui_vocab::SharedString, M>,
        ) -> ReactiveValue<zgui_vocab::SharedString> {
            source.into_reactive_value()
        }
        let label = shared(|| "Pause");
        assert!(!label.is_constant());
        assert_eq!(label.get().as_str(), "Pause");
        assert_eq!(shared(|| String::from("Play")).get().as_str(), "Play");
        assert_eq!(shared("Stop").get().as_str(), "Stop");
    }

    #[test]
    fn a_conversion_happens_at_the_edge() {
        let text: ReactiveValue<String> = "abc".into_reactive_value();
        assert_eq!(text.get(), "abc");
    }
}
