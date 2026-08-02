//! Accessibility properties that change over time.

use std::rc::Rc;

use zgui_vocab::{A11y, Role, Semantics};

use crate::value::{IntoReactiveValue, ReactiveValue};

/// One step in building a [`Semantics`], applied at write time.
type Step = Rc<dyn Fn(A11y) -> A11y>;

/// The reactive accumulator for a view's accessibility properties.
///
/// [`A11y`] is pure data: it holds resolved values, and its own dependency list cannot name a
/// signal at all. That is right for the value that crosses the backend seam, and wrong for
/// everything that has to *accumulate* accessibility properties before they are resolved — a
/// forwarded attribute bundle, a slot's properties, a component merging its caller's over its own.
/// This is that accumulator: the same builder shape, over values that may change, lowering to an
/// `A11y` at the moment of writing, inside the same effect every other reactive attribute uses.
///
/// ```
/// use zgui_reactive::{Mounted, RwSignal, install};
/// use zgui_vocab::{Role, SemanticFlags};
/// use zgui_view::A11yBinding;
///
/// install().unwrap();
/// let node = Mounted::new();
/// let disabled = node.with(|| RwSignal::new(false));
///
/// let a11y = A11yBinding::new(Role::Button).label("Save").disabled(disabled);
/// assert_eq!(a11y.lower().role, Role::Button);
/// assert_eq!(a11y.lower().label.as_deref(), Some("Save"));
///
/// // ... and it says something different once the signal does.
/// zgui_reactive::prelude::Set::set(&disabled, true);
/// assert!(a11y.lower().flags.contains(SemanticFlags::DISABLED));
/// node.unmount();
/// ```
#[derive(Clone)]
pub struct A11yBinding {
    /// What the element is, when anything said.
    role: Option<ReactiveValue<Role>>,
    /// Everything else, applied in the order it was written.
    steps: Vec<Step>,
}

impl A11yBinding {
    /// A binding for an element of this role.
    pub fn new(role: Role) -> Self {
        Self {
            role: Some(ReactiveValue::Constant(role)),
            steps: Vec::new(),
        }
    }

    /// A binding whose role itself changes.
    pub fn with_role<M>(role: impl IntoReactiveValue<Role, M>) -> Self {
        Self {
            role: Some(role.into_reactive_value()),
            steps: Vec::new(),
        }
    }

    /// A binding that says nothing about what the element is.
    ///
    /// It lowers to [`Role::GenericContainer`] on its own, and it defers to whatever role it is
    /// merged over. That distinction is the difference between adding a label to a button and
    /// turning that button into an anonymous box: a caller who writes only `label` names no role,
    /// and a binding that invented one for them would silently discard the component's.
    ///
    /// ```
    /// use zgui_vocab::Role;
    /// use zgui_view::A11yBinding;
    ///
    /// let button = A11yBinding::new(Role::Button).label("Save");
    /// let caller = A11yBinding::unspecified().label("Save changes");
    ///
    /// let merged = button.merged(&caller);
    /// assert_eq!(merged.lower().role, Role::Button);
    /// assert_eq!(merged.lower().label.as_deref(), Some("Save changes"));
    ///
    /// // On its own it is a plain box.
    /// assert_eq!(A11yBinding::unspecified().lower().role, Role::GenericContainer);
    /// ```
    #[must_use]
    pub fn unspecified() -> Self {
        Self {
            role: None,
            steps: Vec::new(),
        }
    }

    /// Adds a step of the caller's own, for a property with no method here yet.
    ///
    /// The closure runs inside the binding's effect, so anything reactive it reads is tracked.
    #[must_use]
    pub fn step(mut self, step: impl Fn(A11y) -> A11y + 'static) -> Self {
        self.steps.push(Rc::new(step));
        self
    }

    /// Appends `other`'s steps after this one's, so `other` wins where they disagree.
    ///
    /// This is how a caller's accessibility properties beat a component's: the caller knows the
    /// context the control sits in, and the component does not. A property `other` never set is
    /// not a disagreement, and that includes its role: see [`A11yBinding::unspecified`].
    #[must_use]
    pub fn merged(mut self, other: &Self) -> Self {
        if let Some(role) = other.role.clone() {
            self.steps
                .push(Rc::new(move |a11y: A11y| a11y.role(role.get())));
        }
        self.steps.extend(other.steps.iter().cloned());
        self
    }

    /// States how large the whole table is, in rows and columns.
    ///
    /// Both together, because a table states its shape or it does not: a row count with no column
    /// count describes a table whose width a reader still has to count for itself.
    ///
    /// A virtualised table needs this and an ordinary one does not. When every row is an element,
    /// a consumer counts them; when thirty rows stand for ten thousand, counting them is wrong, and
    /// this is the only thing that says so.
    #[must_use]
    pub fn table_size<M, N>(
        self,
        rows: impl IntoReactiveValue<usize, M>,
        columns: impl IntoReactiveValue<usize, N>,
    ) -> Self {
        let rows = rows.into_reactive_value();
        let columns = columns.into_reactive_value();
        self.step(move |a11y| a11y.table_size(rows.get(), columns.get()))
    }

    /// Resolves every property now, tracking whatever they read.
    pub fn lower(&self) -> Semantics {
        let role = self
            .role
            .as_ref()
            .map_or(Role::GenericContainer, ReactiveValue::get);
        let mut a11y = A11y::new(role);
        for step in &self.steps {
            a11y = step(a11y);
        }
        a11y.build()
    }
}

/// Declares one builder method per accessibility property.
macro_rules! properties {
    ($( $name:ident : $type:ty => $doc:literal; )+) => {
        impl A11yBinding {
            $(
                #[doc = $doc]
                #[must_use]
                pub fn $name<M>(self, value: impl IntoReactiveValue<$type, M>) -> Self {
                    let value = value.into_reactive_value();
                    self.step(move |a11y| a11y.$name(value.get()))
                }
            )+
        }
    };
}

properties! {
    label: zgui_vocab::SharedString => "What this element is called.";
    description: zgui_vocab::SharedString => "A longer description of this element.";
    value: zgui_vocab::SharedString => "This control's value, as text.";
    placeholder: zgui_vocab::SharedString => "The text shown when this field is empty.";
    role_description: zgui_vocab::SharedString => "What this element's role is called, in words.";
    keyboard_shortcut: zgui_vocab::SharedString => "The keystroke that activates this element.";
    tooltip: zgui_vocab::SharedString => "The tooltip shown for this element.";
    disabled: bool => "Whether this element cannot be interacted with.";
    read_only: bool => "Whether this control's value cannot be changed.";
    required: bool => "Whether this control must have a value.";
    modal: bool => "Whether this element takes the interaction over.";
    busy: bool => "Whether this element is still loading.";
    hidden: bool => "Whether this element is hidden from an assistive technology.";
    expanded: bool => "Whether this element's content is showing.";
    selected: bool => "Whether this element is selected.";
    toggled_on: bool => "Whether this toggle is on.";
    toggled: zgui_vocab::Toggled => "This toggle's three-state position.";
    invalid: zgui_vocab::Invalid => "How this control's value fails its constraints.";
    live: zgui_vocab::Live => "How urgently a change here should be announced.";
    orientation: zgui_vocab::Orientation => "Which way this element is laid out.";
    has_popup: zgui_vocab::HasPopup => "What kind of surface this element opens.";
    numeric_value: f64 => "This control's value, as a number.";
    level: usize => "This heading's level.";
    sort_direction: zgui_vocab::SortDirection => "Which way this column header's column is sorted.";
    current: zgui_vocab::AriaCurrent => "Which item of a set this one is the current one of.";
    state_description: zgui_vocab::SharedString => "What this control's value means, in words.";
    auto_complete: zgui_vocab::AutoComplete => "How this field completes what is typed into it.";
    row_index: usize => "Which row of its table this element is, counting from zero.";
    column_index: usize => "Which column of its table this cell is, counting from zero.";
    row_span: usize => "How many rows this cell covers.";
    column_span: usize => "How many columns this cell covers.";
    labelled_by: zgui_vocab::NodeId => "The element whose text names this one.";
    described_by: zgui_vocab::NodeId => "The element whose text describes this one.";
    controls: zgui_vocab::NodeId => "The element this one controls.";
    owns: zgui_vocab::NodeId => "The element this one owns, which is not its child in the tree.";
    active_descendant: zgui_vocab::NodeId => "The descendant that behaves as though it had focus.";
    popup_for: zgui_vocab::NodeId => "The element this popup belongs to.";
    error_message: zgui_vocab::NodeId => "The element holding this control's error message.";
}

#[cfg(test)]
mod tests {
    use zgui_reactive::prelude::*;
    use zgui_reactive::{Mounted, RwSignal, install};
    use zgui_vocab::{Role, SemanticFlags};

    use super::A11yBinding;

    #[test]
    fn a_reactive_property_is_resolved_at_the_moment_of_lowering() {
        install().ok();
        let node = Mounted::new();
        let busy = node.with(|| RwSignal::new(false));
        let binding = A11yBinding::new(Role::Button).busy(busy);

        assert!(!binding.lower().flags.contains(SemanticFlags::BUSY));
        busy.set(true);
        assert!(binding.lower().flags.contains(SemanticFlags::BUSY));
        node.unmount();
    }

    #[test]
    fn a_merged_binding_lets_the_caller_win() {
        let component = A11yBinding::new(Role::Button).label("Save");
        let caller = A11yBinding::new(Role::Button).label("Save changes");
        let merged = component.merged(&caller);
        assert_eq!(merged.lower().label.as_deref(), Some("Save changes"));
    }

    #[test]
    fn a_merged_binding_keeps_what_the_caller_did_not_set() {
        let component = A11yBinding::new(Role::Button).label("Save").disabled(true);
        // A caller who wrote only a label named no role, so this is the binding the caller's
        // side actually produces — not one that repeats the component's own role back at it.
        let caller = A11yBinding::unspecified().label("Save changes");
        let merged = component.merged(&caller);
        assert_eq!(merged.lower().label.as_deref(), Some("Save changes"));
        assert!(merged.lower().flags.contains(SemanticFlags::DISABLED));
        assert_eq!(
            merged.lower().role,
            Role::Button,
            "a caller who named no role does not turn the control into a box"
        );
    }

    #[test]
    fn a_role_the_caller_did_name_still_wins() {
        let component = A11yBinding::new(Role::Button);
        let caller = A11yBinding::new(Role::Link);
        assert_eq!(component.merged(&caller).lower().role, Role::Link);
    }

    #[test]
    fn a_binding_that_names_no_role_is_a_plain_box_on_its_own() {
        assert_eq!(
            A11yBinding::unspecified().label("x").lower().role,
            Role::GenericContainer
        );
    }

    #[test]
    fn a_reactive_role_is_resolved_too() {
        install().ok();
        let node = Mounted::new();
        let role = node.with(|| RwSignal::new(Role::Button));
        let binding = A11yBinding::with_role(role);

        assert_eq!(binding.lower().role, Role::Button);
        role.set(Role::Link);
        assert_eq!(binding.lower().role, Role::Link);
        node.unmount();
    }
}
