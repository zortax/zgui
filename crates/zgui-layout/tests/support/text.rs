//! Finding the inline formatting contexts in a laid-out store, and the paragraphs to fill them.

use zgui_layout::BoxKey;
use zgui_layout::inline::lines::LineBox;
use zgui_layout::tree::store::LayoutStore;

/// Every box in `store` that established an inline formatting context and resolved to lines.
pub(crate) fn inline_roots(store: &LayoutStore) -> Vec<BoxKey> {
    let mut out = Vec::new();
    let mut stack = vec![store.root().expect("a root")];
    while let Some(key) = stack.pop() {
        if store.inline_resolution(key).is_some() {
            out.push(key);
        }
        stack.extend(store.node(key).children.iter().copied());
    }
    out
}

/// The first inline formatting context in `store`.
pub(crate) fn first_inline_root(store: &LayoutStore) -> BoxKey {
    *inline_roots(store).first().expect("a context")
}

/// The lines the first context in `store` resolved to.
pub(crate) fn lines(store: &LayoutStore) -> Vec<LineBox> {
    store
        .inline_resolution(first_inline_root(store))
        .expect("laid out")
        .lines
        .clone()
}

/// A paragraph of `words` words, long enough to wrap many times at any usual width.
///
/// The words are drawn from a small vocabulary in a fixed rotation, so two paragraphs of the same
/// length are the same text and a paragraph is the same text on every run.
pub(crate) fn paragraph(words: usize) -> String {
    let vocabulary = [
        "alpha", "bravo", "delta", "gamma", "kappa", "sigma", "omega",
    ];
    let mut out = String::new();
    for index in 0..words {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(vocabulary[index % vocabulary.len()]);
    }
    out
}
