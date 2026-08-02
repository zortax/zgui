//! The builder that makes a declaration readable at the call site.

use accesskit::NodeId;

use crate::a11y::enums::{
    AriaCurrent, AutoComplete, HasPopup, Invalid, Live, Orientation, SortDirection, Toggled,
};
use crate::a11y::role::Role;
use crate::a11y::semantics::{SemanticFlags, Semantics};
use crate::text::SharedString;

/// A declaration under construction.
///
/// [`Semantics`] has three dozen fields and almost every element sets three of them, so building
/// one by struct literal means writing `..Default::default()` at every call site and reading past
/// it at every review. This wraps it in a chain that names only what is being said.
///
/// It holds no reactivity of any kind: every value handed to it is already resolved. A caller with
/// a value that changes over time resolves it first and calls this again.
///
/// ```
/// use zgui_vocab::{A11y, Role, Semantics};
///
/// let semantics: Semantics = A11y::new(Role::CheckBox)
///     .label("Remember me")
///     .toggled_on(true)
///     .required(true)
///     .into();
///
/// assert_eq!(semantics.label.as_deref(), Some("Remember me"));
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct A11y {
    semantics: Semantics,
}

impl A11y {
    /// A declaration of an element with the given role.
    pub fn new(role: Role) -> Self {
        Self {
            semantics: Semantics::new(role),
        }
    }

    /// Continues from an existing declaration, so a caller can override part of one.
    pub fn from_semantics(semantics: Semantics) -> Self {
        Self { semantics }
    }

    /// The declaration built so far.
    pub fn build(self) -> Semantics {
        self.semantics
    }

    /// Replaces the role.
    pub fn role(mut self, role: Role) -> Self {
        self.semantics.role = role;
        self
    }

    /// Sets the name announced for this element.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.semantics.label = Some(label.into());
        self
    }

    /// Sets the longer explanation announced after the name.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.semantics.description = Some(description.into());
        self
    }

    /// Sets the element's current value, as text.
    pub fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.semantics.value = Some(value.into());
        self
    }

    /// Sets the text shown in an empty field to say what belongs in it.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.semantics.placeholder = Some(placeholder.into());
        self
    }

    /// Sets a phrase to announce in place of the role's usual name.
    pub fn role_description(mut self, description: impl Into<SharedString>) -> Self {
        self.semantics.role_description = Some(description.into());
        self
    }

    /// Sets the key sequence that operates this element from anywhere.
    pub fn keyboard_shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.semantics.keyboard_shortcut = Some(shortcut.into());
        self
    }

    /// Sets the text of the tip shown when this element is pointed at.
    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.semantics.tooltip = Some(tooltip.into());
        self
    }

    /// Sets or clears one of the on-or-off properties.
    pub fn flag(mut self, flag: SemanticFlags, on: bool) -> Self {
        self.semantics.flags = self.semantics.flags.with(flag, on);
        self
    }

    /// States whether this control can be operated.
    pub fn disabled(self, disabled: bool) -> Self {
        self.flag(SemanticFlags::DISABLED, disabled)
    }

    /// States whether this control's value can be changed.
    pub fn read_only(self, read_only: bool) -> Self {
        self.flag(SemanticFlags::READ_ONLY, read_only)
    }

    /// States whether this control must have a value.
    pub fn required(self, required: bool) -> Self {
        self.flag(SemanticFlags::REQUIRED, required)
    }

    /// States whether nothing outside this element can be interacted with while it is shown.
    pub fn modal(self, modal: bool) -> Self {
        self.flag(SemanticFlags::MODAL, modal)
    }

    /// States whether this element's content is still being produced.
    pub fn busy(self, busy: bool) -> Self {
        self.flag(SemanticFlags::BUSY, busy)
    }

    /// States whether this element and its subtree are absent from the presented tree.
    pub fn hidden(self, hidden: bool) -> Self {
        self.flag(SemanticFlags::HIDDEN, hidden)
    }

    /// States whether this element clips its children.
    pub fn clips_children(self, clips: bool) -> Self {
        self.flag(SemanticFlags::CLIPS_CHILDREN, clips)
    }

    /// States whether this element is expanded, marking it expandable in the first place.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.semantics.expanded = Some(expanded);
        self
    }

    /// States whether this element is selected, marking it selectable in the first place.
    pub fn selected(mut self, selected: bool) -> Self {
        self.semantics.selected = Some(selected);
        self
    }

    /// Sets the three-valued checked state.
    pub fn toggled(mut self, toggled: Toggled) -> Self {
        self.semantics.toggled = Some(toggled);
        self
    }

    /// Sets the checked state from a plain flag, for controls with no mixed state.
    pub fn toggled_on(self, on: bool) -> Self {
        self.toggled(if on { Toggled::True } else { Toggled::False })
    }

    /// States why this control's value is rejected.
    pub fn invalid(mut self, invalid: Invalid) -> Self {
        self.semantics.invalid = Some(invalid);
        self
    }

    /// States how urgently changes inside this region should be announced.
    pub fn live(mut self, live: Live) -> Self {
        self.semantics.live = Some(live);
        self
    }

    /// States which way this control is laid out.
    pub fn orientation(mut self, orientation: Orientation) -> Self {
        self.semantics.orientation = Some(orientation);
        self
    }

    /// States what kind of surface this control opens.
    pub fn has_popup(mut self, popup: HasPopup) -> Self {
        self.semantics.has_popup = Some(popup);
        self
    }

    /// States which way this column header's column is sorted.
    ///
    /// Set on the header, not on the table: sorting is a property of one column, and a reader
    /// announcing "sorted ascending" wants to hear it while it is on the header it could press to
    /// change.
    pub fn sort_direction(mut self, direction: SortDirection) -> Self {
        self.semantics.sort_direction = Some(direction);
        self
    }

    /// States which item of a set this one is the current one of.
    pub fn current(mut self, current: AriaCurrent) -> Self {
        self.semantics.current = Some(current);
        self
    }

    /// States what this control's value means, in words, beside the value itself.
    pub fn state_description(mut self, description: impl Into<SharedString>) -> Self {
        self.semantics.state_description = Some(description.into());
        self
    }

    /// States how this field completes what is typed into it.
    pub fn auto_complete(mut self, auto_complete: AutoComplete) -> Self {
        self.semantics.auto_complete = Some(auto_complete);
        self
    }

    /// States how large the whole table is, in rows and columns.
    ///
    /// Set on the table itself, and needed exactly when the structure does not say: a virtualised
    /// grid holds a window onto its rows, so counting the ones that exist would announce a table of
    /// thirty rows to somebody looking at ten thousand.
    pub fn table_size(mut self, rows: usize, columns: usize) -> Self {
        self.semantics.table.row_count = Some(rows);
        self.semantics.table.column_count = Some(columns);
        self
    }

    /// States which row of its table this element is, counting from zero.
    pub fn row_index(mut self, index: usize) -> Self {
        self.semantics.table.row_index = Some(index);
        self
    }

    /// States which column of its table this cell is, counting from zero.
    pub fn column_index(mut self, index: usize) -> Self {
        self.semantics.table.column_index = Some(index);
        self
    }

    /// States how many rows this cell covers.
    pub fn row_span(mut self, span: usize) -> Self {
        self.semantics.table.row_span = Some(span);
        self
    }

    /// States how many columns this cell covers.
    pub fn column_span(mut self, span: usize) -> Self {
        self.semantics.table.column_span = Some(span);
        self
    }

    /// Sets the current value of a control that measures rather than names.
    pub fn numeric_value(mut self, value: f64) -> Self {
        self.semantics.numeric.value = Some(value);
        self
    }

    /// Sets the range a measuring control accepts.
    pub fn numeric_range(mut self, min: f64, max: f64) -> Self {
        self.semantics.numeric.min = Some(min);
        self.semantics.numeric.max = Some(max);
        self
    }

    /// Sets how far one increment moves a measuring control's value.
    pub fn numeric_step(mut self, step: f64) -> Self {
        self.semantics.numeric.step = Some(step);
        self
    }

    /// States where this element sits in a set a consumer cannot count itself.
    pub fn set_position(mut self, position_in_set: usize, size_of_set: usize) -> Self {
        self.semantics.position.position_in_set = Some(position_in_set);
        self.semantics.position.size_of_set = Some(size_of_set);
        self
    }

    /// States how deep this element is nested, counting from one.
    pub fn level(mut self, level: usize) -> Self {
        self.semantics.position.level = Some(level);
        self
    }

    /// Adds an element whose text names this one.
    pub fn labelled_by(mut self, target: NodeId) -> Self {
        self.semantics.relations.labelled_by.push(target);
        self
    }

    /// Adds an element whose text describes this one.
    pub fn described_by(mut self, target: NodeId) -> Self {
        self.semantics.relations.described_by.push(target);
        self
    }

    /// Adds an element whose content or presence this one governs.
    pub fn controls(mut self, target: NodeId) -> Self {
        self.semantics.relations.controls.push(target);
        self
    }

    /// Adds an element that belongs to this one despite not being its child.
    pub fn owns(mut self, target: NodeId) -> Self {
        self.semantics.relations.owns.push(target);
        self
    }

    /// Adds another member of this element's mutually exclusive group.
    pub fn radio_group(mut self, target: NodeId) -> Self {
        self.semantics.relations.radio_group.push(target);
        self
    }

    /// Names the descendant that is currently active.
    pub fn active_descendant(mut self, target: NodeId) -> Self {
        self.semantics.relations.active_descendant = Some(target);
        self
    }

    /// Names the element this one is the pop-up surface for.
    pub fn popup_for(mut self, target: NodeId) -> Self {
        self.semantics.relations.popup_for = Some(target);
        self
    }

    /// Names the element holding the message explaining why this value is rejected.
    pub fn error_message(mut self, target: NodeId) -> Self {
        self.semantics.relations.error_message = Some(target);
        self
    }
}

impl From<A11y> for Semantics {
    fn from(builder: A11y) -> Self {
        builder.build()
    }
}

impl From<Semantics> for A11y {
    fn from(semantics: Semantics) -> Self {
        Self::from_semantics(semantics)
    }
}

impl From<Role> for A11y {
    fn from(role: Role) -> Self {
        Self::new(role)
    }
}

#[cfg(test)]
mod tests {
    use super::A11y;
    use crate::a11y::enums::{SortDirection, Toggled};
    use crate::a11y::role::Role;
    use crate::a11y::semantics::{SemanticFlags, Semantics};
    use accesskit::NodeId;

    #[test]
    fn a_grid_states_its_true_shape_and_a_cell_its_true_place() {
        // The properties a virtualised grid needs and no structure can supply: thirty rows of
        // elements standing in for ten thousand rows of data.
        let grid = A11y::new(Role::Grid).table_size(10_000, 4).build();
        assert_eq!(grid.table.row_count, Some(10_000));
        assert_eq!(grid.table.column_count, Some(4));

        let cell = A11y::new(Role::Cell)
            .row_index(4_211)
            .column_index(2)
            .row_span(1)
            .column_span(2)
            .build();
        assert_eq!(cell.table.row_index, Some(4_211));
        assert_eq!(cell.table.column_index, Some(2));
        assert_eq!(cell.table.column_span, Some(2));
        assert!(cell.table.is_set());
    }

    #[test]
    fn a_column_header_says_which_way_its_column_is_sorted() {
        let header = A11y::new(Role::ColumnHeader)
            .label("Name")
            .sort_direction(SortDirection::Ascending)
            .build();
        assert_eq!(header.sort_direction, Some(SortDirection::Ascending));
        assert_eq!(
            A11y::new(Role::ColumnHeader).build().sort_direction,
            None,
            "an unsorted column claims no direction rather than claiming the default one",
        );
    }

    #[test]
    fn a_chain_sets_only_what_it_names() {
        let semantics: Semantics = A11y::new(Role::Button).label("Save").into();
        assert_eq!(semantics.role, Role::Button);
        assert_eq!(semantics.label.as_deref(), Some("Save"));
        assert_eq!(semantics.description, None);
        assert!(semantics.flags.is_empty());
    }

    #[test]
    fn a_flag_set_false_stays_absent_from_the_set() {
        let semantics = A11y::new(Role::Button).disabled(false).build();
        assert!(!semantics.flags.contains(SemanticFlags::DISABLED));
        assert!(semantics.flags.is_empty());
    }

    #[test]
    fn continuing_from_a_declaration_overrides_only_the_named_part() {
        let base = A11y::new(Role::Button).label("Save").disabled(true).build();
        let overridden = A11y::from_semantics(base).label("Store").build();
        assert_eq!(overridden.label.as_deref(), Some("Store"));
        assert!(overridden.flags.contains(SemanticFlags::DISABLED));
    }

    #[test]
    fn relations_accumulate_in_the_order_they_are_added() {
        let semantics = A11y::new(Role::TextInput)
            .labelled_by(NodeId(1))
            .labelled_by(NodeId(2))
            .error_message(NodeId(3))
            .build();
        assert_eq!(semantics.relations.labelled_by, vec![NodeId(1), NodeId(2)]);
        assert_eq!(semantics.relations.error_message, Some(NodeId(3)));
    }

    #[test]
    fn the_plain_checked_shorthand_agrees_with_the_three_valued_one() {
        assert_eq!(
            A11y::new(Role::CheckBox).toggled_on(true).build().toggled,
            Some(Toggled::True)
        );
        assert_eq!(
            A11y::new(Role::CheckBox).toggled_on(false).build().toggled,
            Some(Toggled::False)
        );
    }
}
