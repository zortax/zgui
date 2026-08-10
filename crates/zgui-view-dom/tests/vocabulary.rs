//! Every name in the vocabulary, built into a real document and styled by the framework's own
//! sheet.
//!
//! The golden is the layout default each name gets. It is what makes the vocabulary and the sheet
//! one thing rather than two: a name added to the vocabulary and not to the sheet is an element
//! that lays out as an inline run of nothing, and no test that only builds the tree can see it.

mod support;

use std::sync::Arc;

use zgui_css::values::size::{DisplayInside, DisplayOutside};
use zgui_elements::{
    Element, r#box, canvas, column, control, editor, field, image, label, overlay_root, row,
    scroll, spacer, stack, surface, text, vector,
};
use zgui_geom::CssPx;
use zgui_style::{StyleEngine, Viewport};
use zgui_text::FixedMetrics;
use zgui_view::{Anchor, IntoView, View};

use crate::support::Window;

/// The layout default `node` was given, written the way the golden spells it.
fn display_of(window: &Window, node: zgui_view::NodeId) -> String {
    let index = window.backend.index_of(node);
    let document = window.document.borrow();
    let style = document
        .node(index)
        .primary_style()
        .expect("every element in the tree was styled");
    let display = style.get_box().clone_display();
    let outside = match display.outside() {
        DisplayOutside::Block => "block",
        DisplayOutside::Inline => "inline",
        DisplayOutside::None => "none",
        _ => "other",
    };
    let inside = match display.inside() {
        DisplayInside::Flow => "flow",
        DisplayInside::FlowRoot => "flow-root",
        DisplayInside::Flex => "flex",
        DisplayInside::Grid => "grid",
        DisplayInside::None => "none",
        _ => "other",
    };
    format!("{outside} {inside}")
}

/// Builds one element under the window's root and returns the node it made.
fn build<T: zgui_elements::Tag>(window: &Window, element: Element<T>) -> zgui_view::NodeId {
    let mut built = window
        .window
        .with(|| element.into_view().build(&mut window.cx.cx()));
    built.mount(&window.dom, window.root, None);
    built.node()
}

#[test]
fn every_intrinsic_element_builds_a_real_node_and_takes_its_layout_default_from_the_sheet() {
    let window = Window::open();

    // One of each, in the order the vocabulary lists them.
    let built: Vec<(&str, zgui_view::NodeId)> = vec![
        ("box", build(&window, r#box())),
        ("row", build(&window, row())),
        ("column", build(&window, column())),
        ("stack", build(&window, stack())),
        ("text", build(&window, text())),
        ("label", build(&window, label())),
        ("image", build(&window, image())),
        ("vector", build(&window, vector())),
        ("scroll", build(&window, scroll())),
        ("canvas", build(&window, canvas())),
        ("editor", build(&window, editor())),
        ("field", build(&window, field())),
        ("control", build(&window, control())),
        ("surface", build(&window, surface())),
        ("spacer", build(&window, spacer())),
        ("overlay_root", build(&window, overlay_root())),
    ];
    assert_eq!(
        built.len(),
        zgui_elements::names().len(),
        "one element per name in the vocabulary"
    );

    let mut engine = {
        let document = window.document.borrow();
        StyleEngine::new(
            &document,
            Arc::new(FixedMetrics::new()),
            Viewport::new(CssPx(1280.0), CssPx(800.0)),
        )
    };
    {
        let mut document = window.document.borrow_mut();
        let pass = engine.restyle(&mut document, None);
        assert!(pass.styled > 0, "the traversal reached the tree");
    }

    let mut lines = String::new();
    for (name, node) in &built {
        lines.push_str(&format!("{name} {}\n", display_of(&window, *node)));
    }
    let expected = include_str!("goldens/view/element_display_defaults.txt");
    assert_eq!(lines, expected);

    window.window.unmount();
}

/// Which names are born replaced, over the whole vocabulary at once.
///
/// A replaced element holds content the document does not own, so its box is sized from that
/// content and layout never reaches its children. Naming a container here empties it in a way
/// nothing else can see: the frame is styled and painted at the size the sheet gives it, the
/// children are in the tree, the accessibility tree reads them — and the window shows an empty
/// box. Two components of the library above shipped in that state.
///
/// So the list is pinned by name and read off the flag the document actually carries. A name that
/// joins it is an edit here as well as in the backend.
#[test]
fn only_the_names_whose_content_comes_from_outside_are_born_replaced() {
    use zgui_view::Dom;

    let window = Window::open();
    let mut replaced: Vec<&str> = Vec::new();
    for name in zgui_elements::names() {
        let node = window.backend.create_element(name);
        let index = window.backend.index_of(node);
        if window.document.borrow().node(index).replaced_id().is_some() {
            replaced.push(name.as_str());
        }
    }
    replaced.sort_unstable();
    assert_eq!(
        replaced,
        ["image", "surface"],
        "a container that is born replaced draws its own frame and none of its children"
    );
}

/// The vocabulary and the sheet are two lists of the same names, and nothing else compares them.
///
/// The comparison is against the sheet's *selectors*, not against its text. Half the vocabulary
/// appears in the text of a sheet that names none of it — `box` inside `box-sizing`, `row` and
/// `column` inside `flex-direction`, `text` inside `--zgui-selection-text` — so a containment test
/// would go on passing for a sheet that had lost every rule it is written to check.
#[test]
fn the_sheet_gives_every_name_in_the_vocabulary_a_layout_of_its_own() {
    let selected = type_selectors(zgui_style::sheets::ua::USER_AGENT_SHEET);
    for name in zgui_elements::names() {
        assert!(
            selected.contains(&name.as_str().to_owned()),
            "the framework's own style sheet has no rule selecting `{}`, so it would lay out as \
             an inline run of nothing. The sheet selects: {selected:?}",
            name.as_str()
        );
    }
}

/// The converse: the check above is only worth anything if it can fail.
#[test]
fn a_sheet_that_only_mentions_a_name_in_a_property_does_not_select_it() {
    let selected = type_selectors("* { box-sizing: border-box } column { flex-direction: row }");
    assert!(selected.contains(&"column".to_owned()));
    assert!(
        !selected.contains(&"box".to_owned()),
        "`box-sizing` is not a rule about `box`"
    );
    assert!(
        !selected.contains(&"row".to_owned()),
        "`row` is a value here, not a selector"
    );
}

/// Every bare element name `css` selects.
///
/// A selector list is what sits before a `{`; a compound selector made only of identifier
/// characters is a type selector and nothing else, which is exactly the form the vocabulary's
/// layout defaults are written in.
fn type_selectors(css: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = css;
    while let Some(brace) = rest.find('{') {
        let (prelude, tail) = rest.split_at(brace);
        let start = prelude.rfind('}').map_or(0, |end| end + 1);
        for selector in prelude[start..].split(',') {
            let selector = selector.trim();
            if !selector.is_empty()
                && selector
                    .chars()
                    .all(|character| character.is_alphanumeric() || character == '_')
            {
                names.push(selector.to_owned());
            }
        }
        rest = &tail[1..];
    }
    names
}
