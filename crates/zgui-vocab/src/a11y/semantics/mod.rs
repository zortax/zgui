//! What an element declares about itself.

mod flags;
mod numeric;
mod relations;
mod table;

pub use crate::a11y::semantics::flags::SemanticFlags;
pub use crate::a11y::semantics::numeric::{Numeric, SetPosition};
pub use crate::a11y::semantics::relations::Relations;
pub use crate::a11y::semantics::table::TablePosition;

use crate::a11y::enums::{
    AriaCurrent, AutoComplete, HasPopup, Invalid, Live, Orientation, SortDirection, Toggled,
};
use crate::a11y::role::Role;
use crate::text::SharedString;

/// Everything an element declares about what it means.
///
/// This is the value a view writes and a node stores. It is plain data with no reactivity in it,
/// which is what lets the same type be produced by a view, held by a document and consumed by a
/// tree projection without any of those three knowing about the others.
///
/// A default [`Semantics`] declares nothing at all and carries the role of a box that exists for
/// layout, [`Role::GenericContainer`] — which is what most elements are, and saying so is exactly
/// right, because a consumer filters that role out of the tree it presents. Build one with
/// [`A11y`](crate::A11y) rather than by filling in fields, unless a whole value is being replaced.
///
/// ```
/// use zgui_vocab::{A11y, Role, SemanticFlags, Semantics};
///
/// let semantics: Semantics = A11y::new(Role::Button).label("Save").disabled(true).into();
/// assert_eq!(semantics.role, Role::Button);
/// assert_eq!(semantics.label.as_deref(), Some("Save"));
/// assert!(semantics.flags.contains(SemanticFlags::DISABLED));
/// ```
///
/// # Three-valued properties are options, and that is not an accident
///
/// [`Semantics::expanded`] and [`Semantics::selected`] are `Option<bool>` rather than `bool`
/// because "has not said" and "said no" are different statements. An element with no expanded
/// property is not expandable at all and offers no expand control; an element that is explicitly
/// not expanded is collapsed and offers one.
#[derive(Clone, Debug, PartialEq)]
pub struct Semantics {
    /// What kind of control this is.
    pub role: Role,
    /// The element's name, as it should be announced.
    ///
    /// Prefer relating the element to the text that already names it, over repeating that text
    /// here: the two then cannot drift apart.
    pub label: Option<SharedString>,
    /// A longer explanation, announced after the name.
    pub description: Option<SharedString>,
    /// The element's current value, as text.
    pub value: Option<SharedString>,
    /// The text shown in an empty field to say what belongs in it.
    pub placeholder: Option<SharedString>,
    /// A phrase to announce in place of the role's usual name.
    pub role_description: Option<SharedString>,
    /// A phrase to announce in place of the state's usual name.
    pub state_description: Option<SharedString>,
    /// The key sequence that operates this element from anywhere.
    pub keyboard_shortcut: Option<SharedString>,
    /// The single character that operates this element while its container has focus.
    pub access_key: Option<SharedString>,
    /// The text of the tip shown when this element is pointed at.
    pub tooltip: Option<SharedString>,
    /// The properties that are simply on or off.
    pub flags: SemanticFlags,
    /// Whether this element is currently expanded, when it is expandable at all.
    pub expanded: Option<bool>,
    /// Whether this element is currently selected, when it is selectable at all.
    pub selected: Option<bool>,
    /// The three-valued checked state.
    pub toggled: Option<Toggled>,
    /// Why this control's value is rejected, when it is.
    pub invalid: Option<Invalid>,
    /// How urgently changes inside this region should be announced.
    pub live: Option<Live>,
    /// Which way this control is laid out, when that changes how it is operated.
    pub orientation: Option<Orientation>,
    /// What kind of surface this control opens.
    pub has_popup: Option<HasPopup>,
    /// What kind of completion this field offers.
    pub auto_complete: Option<AutoComplete>,
    /// Which way this column is currently sorted.
    pub sort_direction: Option<SortDirection>,
    /// In what sense this element is the current one of its set.
    pub current: Option<AriaCurrent>,
    /// The value of a control that measures rather than names.
    pub numeric: Numeric,
    /// Where this element sits in a set a consumer cannot count itself.
    pub position: SetPosition,
    /// Where this element sits in a table.
    pub table: TablePosition,
    /// Which other elements this one is related to.
    pub relations: Relations,
}

impl Semantics {
    /// The declaration of an element with the given role and nothing else.
    pub fn new(role: Role) -> Self {
        Self {
            role,
            label: None,
            description: None,
            value: None,
            placeholder: None,
            role_description: None,
            state_description: None,
            keyboard_shortcut: None,
            access_key: None,
            tooltip: None,
            flags: SemanticFlags::default(),
            expanded: None,
            selected: None,
            toggled: None,
            invalid: None,
            live: None,
            orientation: None,
            has_popup: None,
            auto_complete: None,
            sort_direction: None,
            current: None,
            numeric: Numeric::default(),
            position: SetPosition::default(),
            table: TablePosition::default(),
            relations: Relations::default(),
        }
    }

    /// Whether this declares nothing beyond a role of `Role::GenericContainer`.
    ///
    /// An element whose declaration is trivial contributes nothing to a presented tree, so this is
    /// the test for "this can be left out entirely".
    ///
    /// ```
    /// use zgui_vocab::{Role, Semantics};
    ///
    /// assert!(Semantics::new(Role::GenericContainer).is_trivial());
    /// assert!(!Semantics::new(Role::Button).is_trivial());
    /// ```
    pub fn is_trivial(&self) -> bool {
        *self == Self::new(Role::GenericContainer)
    }
}

impl Default for Semantics {
    /// A declaration that says nothing, on a box that exists for layout.
    ///
    /// The role is [`Role::GenericContainer`] rather than the role enumeration's own default,
    /// which is `Role::Unknown`. The difference is not cosmetic: a consumer drops a generic
    /// container from the tree it presents and keeps an unknown node, so a default carrying the
    /// latter would announce every layout box in the document.
    ///
    /// ```
    /// use zgui_vocab::{Role, Semantics};
    ///
    /// assert_eq!(Semantics::default().role, Role::GenericContainer);
    /// assert!(Semantics::default().is_trivial());
    /// ```
    fn default() -> Self {
        Self::new(Role::GenericContainer)
    }
}

#[cfg(test)]
mod tests {
    use super::{Numeric, Relations, SemanticFlags, Semantics, SetPosition, TablePosition};
    use crate::a11y::Role;
    use accesskit::NodeId;

    #[test]
    fn the_default_declaration_is_the_one_a_consumer_drops() {
        // A layout box built without saying anything must not reach a presented tree, and the
        // role that achieves that is the container role rather than the enumeration's own
        // default. The builder's default has to agree, because it is the same value.
        assert_eq!(Semantics::default().role, Role::GenericContainer);
        assert!(Semantics::default().is_trivial());
        assert_eq!(crate::A11y::default().build(), Semantics::default());
    }

    #[test]
    fn a_fresh_declaration_says_nothing() {
        let semantics = Semantics::new(Role::GenericContainer);
        assert!(semantics.is_trivial());
        assert!(semantics.flags.is_empty());
        assert!(semantics.relations.is_empty());
        assert!(!semantics.numeric.is_set());
        assert!(!semantics.position.is_set());
        assert!(!semantics.table.is_set());
    }

    #[test]
    fn any_single_declaration_makes_it_non_trivial() {
        let with_label = Semantics {
            label: Some("Save".into()),
            ..Semantics::new(Role::GenericContainer)
        };
        assert!(!with_label.is_trivial());

        let with_relation = Semantics {
            relations: Relations {
                popup_for: Some(NodeId(1)),
                ..Relations::default()
            },
            ..Semantics::new(Role::GenericContainer)
        };
        assert!(!with_relation.is_trivial());

        let with_flag = Semantics {
            flags: SemanticFlags::BUSY,
            ..Semantics::new(Role::GenericContainer)
        };
        assert!(!with_flag.is_trivial());
    }

    #[test]
    fn the_three_valued_properties_distinguish_absent_from_false() {
        let unspecified = Semantics::new(Role::Button);
        assert_eq!(unspecified.expanded, None);
        assert_eq!(unspecified.selected, None);

        let collapsed = Semantics {
            expanded: Some(false),
            ..Semantics::new(Role::Button)
        };
        assert_ne!(collapsed.expanded, unspecified.expanded);
    }

    #[test]
    fn the_grouped_properties_are_ordinary_fields() {
        let semantics = Semantics {
            numeric: Numeric {
                value: Some(0.5),
                ..Numeric::default()
            },
            position: SetPosition {
                level: Some(2),
                ..SetPosition::default()
            },
            table: TablePosition {
                row_index: Some(1),
                ..TablePosition::default()
            },
            ..Semantics::new(Role::Slider)
        };
        assert!(semantics.numeric.is_set());
        assert!(semantics.position.is_set());
        assert!(semantics.table.is_set());
    }
}
