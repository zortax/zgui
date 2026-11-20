//! The element that receives composed text.
//!
//! A canvas takes key events and nothing else. An input method, a dead key and the keyboard of a
//! phone all write into an editable element, so each surface has one: a text area the person
//! does not see, placed at the caret so that the input method opens its candidates there. While
//! a field of the application is being typed into, that element has the keyboard in place of the
//! canvas, and what is written into it reaches the application as input-method events.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::{HtmlCanvasElement, HtmlElement, HtmlTextAreaElement};
#[cfg(target_arch = "wasm32")]
use zgui_platform::TextInput;
use zgui_platform::TextInputPurpose;

/// Creates the hidden text area of `canvas`, beside it in the page.
#[cfg(target_arch = "wasm32")]
pub(crate) fn create(canvas: &HtmlCanvasElement) -> Option<HtmlTextAreaElement> {
    let document = canvas.owner_document()?;
    let area: HtmlTextAreaElement = document.create_element("textarea").ok()?.dyn_into().ok()?;
    for (name, value) in [
        ("autocomplete", "off"),
        ("autocorrect", "off"),
        ("autocapitalize", "off"),
        ("spellcheck", "false"),
        ("aria-hidden", "true"),
        ("tabindex", "-1"),
        ("rows", "1"),
    ] {
        let _ = area.set_attribute(name, value);
    }
    let style = area.style();
    for (name, value) in [
        ("position", "fixed"),
        ("width", "1px"),
        ("padding", "0"),
        ("border", "0"),
        ("margin", "0"),
        ("outline", "none"),
        ("resize", "none"),
        ("overflow", "hidden"),
        ("opacity", "0"),
        ("pointer-events", "none"),
        ("caret-color", "transparent"),
        ("background", "transparent"),
        ("color", "transparent"),
        ("white-space", "pre"),
        // Below this size a phone zooms the page to the focused field.
        ("font-size", "16px"),
    ] {
        let _ = style.set_property(name, value);
    }
    canvas.parent_node()?.append_child(&area).ok()?;
    Some(area)
}

/// Moves the text area to the caret and tells the keyboard what is being typed.
#[cfg(target_arch = "wasm32")]
pub(crate) fn place(area: &HtmlTextAreaElement, canvas: &HtmlCanvasElement, state: TextInput) {
    let bounds = canvas.get_bounding_client_rect();
    let style = area.style();
    let pixels = |value: f64| format!("{value:.2}px");
    let _ = style.set_property(
        "left",
        &pixels(bounds.left() + f64::from(state.caret_origin.x.0)),
    );
    let _ = style.set_property(
        "top",
        &pixels(bounds.top() + f64::from(state.caret_origin.y.0)),
    );
    let _ = style.set_property(
        "height",
        &pixels(f64::from(state.caret_size.height.0).max(1.0)),
    );
    let _ = area.set_attribute("inputmode", input_mode(state.purpose));
}

/// The keyboard a phone shows for a purpose.
pub(crate) const fn input_mode(purpose: TextInputPurpose) -> &'static str {
    match purpose {
        TextInputPurpose::Number | TextInputPurpose::Pin => "decimal",
        TextInputPurpose::Phone => "tel",
        TextInputPurpose::Url => "url",
        TextInputPurpose::Email => "email",
        TextInputPurpose::Search => "search",
        _ => "text",
    }
}

/// Gives `element` the keyboard and leaves the page where it is scrolled.
#[cfg(target_arch = "wasm32")]
pub(crate) fn focus(element: &HtmlElement) {
    let options = web_sys::FocusOptions::new();
    options.set_prevent_scroll(true);
    let _ = element.focus_with_options(&options);
}

/// What a browser's input event asks the application to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Edit {
    /// Insert this text.
    Insert(String),
    /// Press the enter key.
    Enter,
    /// Press the backspace key.
    Backspace,
    /// Press the delete key.
    Delete,
}

/// The edit an input event of `kind` with `data` asks for.
///
/// The composition kinds are absent: a composition arrives through its own events. A paste is
/// absent too, because the paste event already carried it.
pub(crate) fn edit(kind: &str, data: Option<String>) -> Option<Edit> {
    match kind {
        "insertText" | "insertReplacementText" => {
            data.filter(|text| !text.is_empty()).map(Edit::Insert)
        }
        "insertLineBreak" | "insertParagraph" => Some(Edit::Enter),
        "deleteContentBackward" => Some(Edit::Backspace),
        "deleteContentForward" => Some(Edit::Delete),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Edit, edit, input_mode};
    use zgui_platform::TextInputPurpose;

    #[test]
    fn typed_text_is_inserted_and_an_empty_insert_asks_for_nothing() {
        assert_eq!(
            edit("insertText", Some("a".to_owned())),
            Some(Edit::Insert("a".to_owned()))
        );
        assert_eq!(edit("insertText", Some(String::new())), None);
        assert_eq!(edit("insertText", None), None);
    }

    #[test]
    fn a_deletion_and_a_line_break_are_keys() {
        assert_eq!(edit("deleteContentBackward", None), Some(Edit::Backspace));
        assert_eq!(edit("deleteContentForward", None), Some(Edit::Delete));
        assert_eq!(edit("insertLineBreak", None), Some(Edit::Enter));
    }

    #[test]
    fn a_composition_and_a_paste_arrive_through_their_own_events() {
        assert_eq!(edit("insertCompositionText", Some("に".to_owned())), None);
        assert_eq!(edit("insertFromPaste", Some("x".to_owned())), None);
    }

    #[test]
    fn a_purpose_selects_the_keyboard_a_phone_shows() {
        assert_eq!(input_mode(TextInputPurpose::Email), "email");
        assert_eq!(input_mode(TextInputPurpose::Number), "decimal");
        assert_eq!(input_mode(TextInputPurpose::Normal), "text");
    }
}
