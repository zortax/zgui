//! Every change to a document goes through its own batch API.
//!
//! The batch is not a convenience wrapper over the arena. A change owes the style engine four
//! things besides itself — a record of what the element was before, a mark on every ancestor so the
//! traversal descends to it, the narrowest hint that is provably enough, and a request for the
//! frame that will show it — and every one of them silently does nothing when it is left out. A
//! write that reached the arena directly would apply, look right in a test that reads the arena
//! back, and produce an interface that never updates.
//!
//! So the backends that drive a document are held to the batch by this check as well as by
//! visibility: the exclusive accessors are what a direct write needs, and naming one of them is
//! what this refuses.

use std::collections::BTreeSet;

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;

/// The crates that drive a document from outside it and must go through its batch API.
const DRIVERS: &[&str] = &["zgui-view-dom", "zgui-input", "zgui-runtime"];

/// The accessors that hand out exclusive access to a document's storage.
///
/// Each is the last step before a write that owes nothing. They are spelled with their opening
/// parenthesis so that the word appearing in prose is not a violation.
const EXCLUSIVE: &[&str] = &[
    "store_mut(",
    "columns_mut(",
    "arena_mut(",
    "store_exclusive(",
];

/// The document's own tree-building methods, which apply a change and owe nothing for it.
///
/// They exist so that a document can be assembled before anything is watching it — the crate's own
/// tests use them, and so does anything reading a document in from somewhere else. Reached from a
/// crate that drives a *live* document they are the same hazard as the accessors above: the write
/// lands, the arena reads back correctly, and the interface never updates.
///
/// None of these shares a name with a method of the batch, so naming one is unambiguous.
const DIRECT: &[&str] = &[".append(", ".append_in(", ".detached(", ".set_flags("];

/// Methods the document and the batch both have, which are a violation only off the batch.
///
/// `edit.set_attribute(…)` is the protocol; `document.set_attribute(…)` is the write that skips
/// it. The two are told apart by what they are called on, which is the whole of the difference.
const SHARED: &[&str] = &[
    ".set_attribute(",
    ".set_classes(",
    ".set_id(",
    ".set_state(",
];

/// What a batch is called at the sites that legitimately use these names.
const BATCH: &[&str] = &["edit.", "edit .", "batch."];

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let mut checked = BTreeSet::new();

    for member in &tree.members {
        if !DRIVERS.contains(&member.name.as_str()) {
            continue;
        }
        checked.insert(member.name.clone());
        for source in &member.sources {
            for (line, text) in source.text.lines().enumerate() {
                let Some(named) = named_write(text) else {
                    continue;
                };
                report.violation(
                    source.rel_path.clone(),
                    format!(
                        "line {}: `{}` writes a document outside its batch; every change goes \
                         through `Document::edit`, which is what records the element's previous \
                         state, marks the ancestors and asks for a frame",
                        line + 1,
                        named.trim_start_matches('.').trim_end_matches('(')
                    ),
                );
            }
        }
    }

    for name in DRIVERS {
        if !checked.contains(*name) {
            report.skip(format!("`{name}` is not in this tree yet"));
        }
    }
    report
}

/// The write `text` makes outside a batch, if it makes one.
fn named_write(text: &str) -> Option<&'static str> {
    if let Some(named) = EXCLUSIVE.iter().find(|accessor| text.contains(*accessor)) {
        return Some(named);
    }
    if let Some(named) = DIRECT.iter().find(|method| text.contains(*method)) {
        return Some(named);
    }
    let shared = SHARED.iter().find(|method| text.contains(*method))?;
    // The same method name on the batch is the protocol rather than a way around it.
    let on_batch = BATCH.iter().any(|receiver| {
        text.split(receiver)
            .skip(1)
            .any(|rest| rest.starts_with(shared.trim_start_matches('.')))
    });
    (!on_batch).then_some(shared)
}

#[cfg(test)]
mod tests {
    use super::named_write;

    #[test]
    fn the_batchs_own_methods_are_not_violations_and_the_documents_twins_are() {
        assert_eq!(named_write("edit.set_attribute(node, name, value);"), None);
        assert_eq!(
            named_write("document.set_attribute(index, name, value);"),
            Some(".set_attribute(")
        );
    }

    #[test]
    fn a_direct_tree_build_is_a_violation_however_it_is_spelled() {
        assert_eq!(
            named_write("let root = document.append(parent, kind, name);"),
            Some(".append(")
        );
        assert_eq!(named_write("edit.create_element(name)"), None);
    }

    #[test]
    fn the_word_in_prose_is_not_a_violation() {
        assert_eq!(
            named_write("// a change may not use store_mut at all"),
            None
        );
    }
}
