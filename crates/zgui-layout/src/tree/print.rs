//! Rendering the box tree and its results as stable, indented, diffable text.
//!
//! Two properties make this useful as evidence rather than as decoration. The same tree renders the
//! same bytes every time — every number goes through one formatter, and siblings are written in the
//! order the tree holds them rather than in whatever order a map iterates. And a field at its
//! default value is left out, so an absent field means the default and nothing else, which keeps a
//! dump of an ordinary document short enough to read.
//!
//! The text is produced here rather than written through the shared writer because that writer
//! belongs to the test harness, and an engine that depended on its own test harness could not be
//! brought up without it. What crosses the seam is the finished, already-indented text.

use core::fmt::Write;

use zgui_dom::side::BoxKey;

use crate::tree::store::{LayoutStore, ResolvedLayout};

/// How many spaces one level of nesting costs.
const INDENT: usize = 2;

/// How many decimal places a number is rendered to.
const DECIMALS: usize = 2;

/// The whole box tree and its resolved layout, as text.
///
/// A store with no root renders as a single line saying so, which is a different rendering from an
/// empty one and therefore cannot be mistaken for it.
pub fn to_text(store: &LayoutStore) -> String {
    let mut out = String::new();
    match store.root() {
        None => out.push_str("no root\n"),
        Some(root) => write_box(store, root, 0, &mut out),
    }
    out
}

/// One box and everything below it.
fn write_box(store: &LayoutStore, key: BoxKey, depth: usize, out: &mut String) {
    let Some(node) = store.get(key) else {
        indent(depth, out);
        let _ = writeln!(out, "<dangling {}>", key.index());
        return;
    };
    indent(depth, out);
    out.push_str(node.kind.label());
    let _ = write!(out, " fc={}", node.fc.label());
    if let Some(pseudo) = node.pseudo {
        let _ = write!(out, " pseudo={}", pseudo.label());
    }
    if let Some(text) = &node.text {
        let _ = write!(out, " text={:?}", &**text);
    }
    if let Some(layout) = store.layout_of(key) {
        write_layout(&layout, out);
    }
    if node.children.len() != node.paint_children.len()
        || node.children.iter().ne(node.paint_children.iter())
    {
        // The two orders differ only where `order` moved something, so saying nothing when they
        // agree keeps the difference visible where it happens.
        let _ = write!(
            out,
            " paint-order=[{}]",
            indices(store, &node.paint_children)
        );
    }
    out.push('\n');
    for &child in &node.children {
        write_box(store, child, depth + 1, out);
    }
}

/// One box's resolved geometry, with every field at its default left out.
fn write_layout(layout: &ResolvedLayout, out: &mut String) {
    let _ = write!(
        out,
        " at=({}, {}) size=({} x {})",
        number(layout.origin.x.0),
        number(layout.origin.y.0),
        number(layout.size.width.0),
        number(layout.size.height.0)
    );
    if layout.content_size.width.0 != layout.size.width.0
        || layout.content_size.height.0 != layout.size.height.0
    {
        let _ = write!(
            out,
            " content=({} x {})",
            number(layout.content_size.width.0),
            number(layout.content_size.height.0)
        );
    }
    for (name, edges) in [("border", layout.border), ("padding", layout.padding)] {
        if [edges.top.0, edges.right.0, edges.bottom.0, edges.left.0]
            .iter()
            .any(|value| *value != 0.0)
        {
            let _ = write!(
                out,
                " {name}=({} {} {} {})",
                number(edges.top.0),
                number(edges.right.0),
                number(edges.bottom.0),
                number(edges.left.0)
            );
        }
    }
    if let Some(baseline) = layout.first_baseline {
        let _ = write!(out, " baseline={}", number(baseline.0));
    }
    if let Some(baseline) = layout
        .last_baseline
        .filter(|baseline| Some(*baseline) != layout.first_baseline)
    {
        let _ = write!(out, " last-baseline={}", number(baseline.0));
    }
}

/// The slot numbers of a child list, for showing an order that differs from the layout one.
fn indices(store: &LayoutStore, children: &[BoxKey]) -> String {
    let _ = store;
    children
        .iter()
        .map(|key| key.index().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// One number, rendered the same way on every machine.
fn number(value: f32) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf".to_owned()
        } else {
            "inf".to_owned()
        };
    }
    // A negative zero is the same number as a positive one and has to print the same, or a box that
    // reached the origin by subtraction diffs against one that started there.
    let value = if value == 0.0 { 0.0 } else { value };
    let mut text = format!("{value:.DECIMALS$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" { "0".to_owned() } else { text }
}

/// Writes one level's worth of indentation.
fn indent(depth: usize, out: &mut String) {
    for _ in 0..depth * INDENT {
        out.push(' ');
    }
}

#[cfg(test)]
mod tests {
    use zgui_arena::DocumentId;

    use crate::tree::store::LayoutStore;

    use super::{number, to_text};

    #[test]
    fn a_store_with_no_root_says_so() {
        let store = LayoutStore::new(DocumentId::FIRST);
        assert_eq!(to_text(&store), "no root\n");
    }

    #[test]
    fn numbers_render_the_same_however_they_were_reached() {
        assert_eq!(number(1.0), "1");
        assert_eq!(number(-0.0), "0");
        assert_eq!(number(0.1 + 0.2), "0.3");
        assert_eq!(number(f32::NAN), "nan");
    }
}
