//! An accessibility node in the vocabulary of a page.
//!
//! A screen reader in a browser reads the page's elements and nothing else, so each node of the
//! application's accessibility tree becomes one element with a role and its states. This module
//! is the dictionary: which ARIA role a node has, and which attributes state what it is.

use accesskit::{Action, HasPopup, Invalid, Live, Node, Orientation, Role, Toggled};

/// The ARIA role of a node, or `None` for a node that is only a container or a run of text.
pub fn role(role: Role) -> Option<&'static str> {
    Some(match role {
        Role::Button | Role::DefaultButton | Role::DisclosureTriangle => "button",
        Role::Link => "link",
        Role::CheckBox => "checkbox",
        Role::RadioButton => "radio",
        Role::RadioGroup => "radiogroup",
        Role::Switch => "switch",
        Role::TextInput
        | Role::MultilineTextInput
        | Role::DateInput
        | Role::DateTimeInput
        | Role::WeekInput
        | Role::MonthInput
        | Role::TimeInput
        | Role::EmailInput
        | Role::PasswordInput
        | Role::PhoneNumberInput
        | Role::UrlInput => "textbox",
        Role::SearchInput => "searchbox",
        Role::NumberInput | Role::SpinButton => "spinbutton",
        Role::ComboBox | Role::EditableComboBox => "combobox",
        Role::ListBox | Role::MenuListPopup => "listbox",
        Role::ListBoxOption | Role::MenuListOption => "option",
        Role::Menu => "menu",
        Role::MenuBar => "menubar",
        Role::MenuItem => "menuitem",
        Role::MenuItemCheckBox => "menuitemcheckbox",
        Role::MenuItemRadio => "menuitemradio",
        Role::Tab => "tab",
        Role::TabList => "tablist",
        Role::TabPanel => "tabpanel",
        Role::Tree => "tree",
        Role::TreeItem => "treeitem",
        Role::TreeGrid => "treegrid",
        Role::List | Role::DescriptionList => "list",
        Role::ListItem => "listitem",
        Role::Table | Role::LayoutTable => "table",
        Role::Grid | Role::ListGrid => "grid",
        Role::Row | Role::LayoutTableRow => "row",
        Role::RowGroup => "rowgroup",
        Role::Cell | Role::LayoutTableCell => "cell",
        Role::GridCell => "gridcell",
        Role::ColumnHeader => "columnheader",
        Role::RowHeader => "rowheader",
        Role::Heading => "heading",
        Role::Image => "img",
        Role::Figure => "figure",
        Role::Dialog => "dialog",
        Role::AlertDialog => "alertdialog",
        Role::Alert => "alert",
        Role::Status => "status",
        Role::Log => "log",
        Role::Timer => "timer",
        Role::Marquee => "marquee",
        Role::Tooltip => "tooltip",
        Role::Slider => "slider",
        Role::ScrollBar => "scrollbar",
        Role::ProgressIndicator => "progressbar",
        Role::Meter => "meter",
        Role::Splitter => "separator",
        Role::Toolbar => "toolbar",
        Role::Group | Role::Details => "group",
        Role::Region | Role::Pane | Role::Section => "region",
        Role::Navigation => "navigation",
        Role::Main => "main",
        Role::Banner | Role::Header => "banner",
        Role::ContentInfo | Role::Footer => "contentinfo",
        Role::Complementary => "complementary",
        Role::Search => "search",
        Role::Form => "form",
        Role::Article => "article",
        Role::Document => "document",
        Role::Application => "application",
        Role::Feed => "feed",
        Role::Note => "note",
        Role::Math => "math",
        Role::Definition => "definition",
        Role::Term => "term",
        Role::Paragraph => "paragraph",
        Role::Blockquote => "blockquote",
        Role::Caption | Role::FigureCaption => "caption",
        Role::Code => "code",
        Role::Emphasis => "emphasis",
        Role::Strong => "strong",
        Role::Time => "time",
        _ => return None,
    })
}

/// The ARIA role of the element of `node`.
///
/// A node with a role of its own has that role. A node with none is read by what it does and
/// what it holds, because a screen reader passes over a name on an element with no role: a node
/// that can be pressed is a button, and a named node that holds other nodes is a group. A named
/// node that holds nothing is its text, and has no role.
pub fn element_role(node: &Node) -> Option<&'static str> {
    role(node.role()).or_else(|| {
        if is_text(node.role()) {
            None
        } else if node.supports_action(Action::Click) {
            Some("button")
        } else if !node.children().is_empty() && node.label().is_some_and(|label| !label.is_empty())
        {
            Some("group")
        } else {
            None
        }
    })
}

/// Whether a node of `role` names itself with the text it holds.
///
/// A run of text and a paragraph are their text. A control has a name, which is an attribute, and
/// may hold other nodes.
pub fn is_text(role: Role) -> bool {
    matches!(
        role,
        Role::Label
            | Role::TextRun
            | Role::Paragraph
            | Role::Heading
            | Role::Caption
            | Role::FigureCaption
            | Role::Code
            | Role::Emphasis
            | Role::Strong
            | Role::Blockquote
            | Role::Term
            | Role::Definition
            | Role::Time
            | Role::Abbr
            | Role::Mark
            | Role::Legend
    )
}

/// Every state attribute the mirror may set, so that a state that ended is taken off the element.
pub const STATES: [&str; 22] = [
    "aria-label",
    "aria-description",
    "aria-placeholder",
    "aria-valuetext",
    "aria-disabled",
    "aria-hidden",
    "aria-expanded",
    "aria-selected",
    "aria-checked",
    "aria-pressed",
    "aria-valuenow",
    "aria-valuemin",
    "aria-valuemax",
    "aria-level",
    "aria-live",
    "aria-modal",
    "aria-readonly",
    "aria-required",
    "aria-busy",
    "aria-haspopup",
    "aria-orientation",
    "aria-invalid",
];

/// The state attributes of a node, each with its value.
pub fn states(node: &Node) -> Vec<(&'static str, String)> {
    let mut states = Vec::new();
    let mut state = |name: &'static str, value: &str| states.push((name, value.to_owned()));
    let text = is_text(node.role());
    if element_role(node).is_some()
        && let Some(label) = node.label().filter(|label| !label.is_empty())
    {
        state("aria-label", label);
    }
    if let Some(description) = node.description().filter(|text| !text.is_empty()) {
        state("aria-description", description);
    }
    if let Some(placeholder) = node.placeholder().filter(|text| !text.is_empty()) {
        state("aria-placeholder", placeholder);
    }
    // A control with a range states its value as a number. Any other control states it as text.
    match node.numeric_value() {
        Some(value) => {
            state("aria-valuenow", &value.to_string());
            if let Some(text) = node.value() {
                state("aria-valuetext", text);
            }
        }
        None if !text && !holds_its_value(node.role()) => {
            if let Some(value) = node.value().filter(|value| !value.is_empty()) {
                state("aria-valuetext", value);
            }
        }
        None => {}
    }
    if let Some(minimum) = node.min_numeric_value() {
        state("aria-valuemin", &minimum.to_string());
    }
    if let Some(maximum) = node.max_numeric_value() {
        state("aria-valuemax", &maximum.to_string());
    }
    if node.is_disabled() {
        state("aria-disabled", "true");
    }
    if node.is_hidden() {
        state("aria-hidden", "true");
    }
    if let Some(expanded) = node.is_expanded() {
        state("aria-expanded", truth(expanded));
    }
    if let Some(selected) = node.is_selected() {
        state("aria-selected", truth(selected));
    }
    if let Some(toggled) = node.toggled() {
        let value = match toggled {
            Toggled::False => "false",
            Toggled::True => "true",
            Toggled::Mixed => "mixed",
        };
        // A button that stays down is pressed. Every other toggle is checked.
        let name = if element_role(node) == Some("button") {
            "aria-pressed"
        } else {
            "aria-checked"
        };
        state(name, value);
    }
    if let Some(level) = node.level() {
        state("aria-level", &level.to_string());
    }
    match node.live() {
        Some(Live::Polite) => state("aria-live", "polite"),
        Some(Live::Assertive) => state("aria-live", "assertive"),
        _ => {}
    }
    if node.is_modal() {
        state("aria-modal", "true");
    }
    if node.is_read_only() {
        state("aria-readonly", "true");
    }
    if node.is_required() {
        state("aria-required", "true");
    }
    if node.is_busy() {
        state("aria-busy", "true");
    }
    if let Some(popup) = node.has_popup() {
        state(
            "aria-haspopup",
            match popup {
                HasPopup::Menu => "menu",
                HasPopup::Listbox => "listbox",
                HasPopup::Tree => "tree",
                HasPopup::Grid => "grid",
                HasPopup::Dialog => "dialog",
            },
        );
    }
    match node.orientation() {
        Some(Orientation::Horizontal) => state("aria-orientation", "horizontal"),
        Some(Orientation::Vertical) => state("aria-orientation", "vertical"),
        None => {}
    }
    match node.invalid() {
        Some(Invalid::True) => state("aria-invalid", "true"),
        Some(Invalid::Grammar) => state("aria-invalid", "grammar"),
        Some(Invalid::Spelling) => state("aria-invalid", "spelling"),
        None => {}
    }
    states
}

/// The text an element of a node holds, when the node has no children to hold it.
///
/// A run of text holds its text, and so does a named node with no role. A field holds what was
/// typed into it, which is what a screen reader reads as the field's value.
pub fn text(node: &Node) -> Option<&str> {
    if is_text(node.role()) || element_role(node).is_none() {
        return node.value().or_else(|| node.label());
    }
    if holds_its_value(node.role()) {
        return node.value();
    }
    None
}

/// Whether the value of a node of `role` is the text inside its element.
fn holds_its_value(role: Role) -> bool {
    matches!(self::role(role), Some("textbox" | "searchbox"))
}

/// A boolean as an ARIA attribute states it.
const fn truth(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

#[cfg(test)]
mod tests {
    use super::{element_role, role, states, text};
    use accesskit::{Action, Node, NodeId, Role, Toggled};

    fn stated(node: &Node, name: &str) -> Option<String> {
        states(node)
            .into_iter()
            .find(|(stated, _)| *stated == name)
            .map(|(_, value)| value)
    }

    #[test]
    fn a_control_has_a_role_and_a_container_has_none() {
        assert_eq!(role(Role::Button), Some("button"));
        assert_eq!(role(Role::MultilineTextInput), Some("textbox"));
        assert_eq!(role(Role::GenericContainer), None);
        assert_eq!(role(Role::Label), None);
    }

    #[test]
    fn a_control_is_named_by_an_attribute_and_text_by_its_content() {
        let mut button = Node::new(Role::Button);
        button.set_label("Save");
        assert_eq!(stated(&button, "aria-label").as_deref(), Some("Save"));
        assert_eq!(text(&button), None);

        let mut label = Node::new(Role::Label);
        label.set_value("Saved");
        assert_eq!(stated(&label, "aria-label"), None);
        assert_eq!(text(&label), Some("Saved"));
    }

    #[test]
    fn a_node_with_no_role_is_read_by_what_it_does_and_what_it_holds() {
        let mut plain = Node::new(Role::GenericContainer);
        plain.set_label("Count");
        assert_eq!(element_role(&plain), None);
        assert_eq!(stated(&plain, "aria-label"), None);
        assert_eq!(text(&plain), Some("Count"));

        let mut pressable = plain.clone();
        pressable.add_action(Action::Click);
        assert_eq!(element_role(&pressable), Some("button"));
        assert_eq!(stated(&pressable, "aria-label").as_deref(), Some("Count"));
        assert_eq!(text(&pressable), None);

        let mut holder = plain;
        holder.push_child(NodeId(7));
        assert_eq!(element_role(&holder), Some("group"));
    }

    #[test]
    fn a_toggle_is_checked_and_a_button_that_stays_down_is_pressed() {
        let mut check = Node::new(Role::CheckBox);
        check.set_toggled(Toggled::Mixed);
        assert_eq!(stated(&check, "aria-checked").as_deref(), Some("mixed"));

        let mut button = Node::new(Role::Button);
        button.set_toggled(Toggled::True);
        assert_eq!(stated(&button, "aria-pressed").as_deref(), Some("true"));
        assert_eq!(stated(&button, "aria-checked"), None);
    }

    #[test]
    fn a_field_holds_its_value_and_a_range_states_it_as_a_number() {
        let mut field = Node::new(Role::TextInput);
        field.set_value("Ada");
        assert_eq!(text(&field), Some("Ada"));
        assert_eq!(stated(&field, "aria-valuetext"), None);

        let mut slider = Node::new(Role::Slider);
        slider.set_numeric_value(3.0);
        slider.set_min_numeric_value(0.0);
        slider.set_max_numeric_value(10.0);
        assert_eq!(stated(&slider, "aria-valuenow").as_deref(), Some("3"));
        assert_eq!(stated(&slider, "aria-valuemax").as_deref(), Some("10"));
    }

    #[test]
    fn every_state_the_mirror_sets_is_one_it_can_take_off() {
        let mut node = Node::new(Role::Slider);
        node.set_label("Volume");
        node.set_numeric_value(1.0);
        node.set_disabled();
        node.set_expanded(true);
        for (name, _) in states(&node) {
            assert!(super::STATES.contains(&name), "{name} is never cleared");
        }
    }
}
