//! A record of every animation lifecycle event the frame dispatched.
//!
//! The half of a stuck-modal investigation that cannot be seen from a view: content kept mounted
//! for an exit animation hears an end through a listener on its own element, so an end delivered
//! to a *different* element — one the content was rebuilt away from — produces no listener call at
//! all and therefore no evidence. This writes the edge as the frame sends it, before any listener
//! has had a chance to be missing.
//!
//! Off unless `ZGUI_MODAL_TRACE` is set in the environment. Every line begins `ZMT `, matching the
//! lines the component library writes, so one session's output reads as one sequence.

use std::cell::Cell;
use std::time::{SystemTime, UNIX_EPOCH};

use zgui_dom::NodeKey;

thread_local! {
    /// Whether the trace is on, read from the environment once.
    static ON: Cell<Option<bool>> = const { Cell::new(None) };
}

/// Microseconds on the wall clock, which is the clock the component library's lines carry too.
fn micros() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
}

/// Whether trace lines are being written.
pub(crate) fn on() -> bool {
    ON.with(|held| match held.get() {
        Some(answer) => answer,
        None => {
            let answer = std::env::var_os("ZGUI_MODAL_TRACE").is_some();
            held.set(Some(answer));
            answer
        }
    })
}

/// Records one lifecycle edge, and how many listeners the dispatch found for it.
///
/// `steps` is the whole of the question this exists to answer: an end delivered to an element with
/// no listener on it resolves to none, and content waiting for that end waits for ever.
pub(crate) fn edge(kind: zgui_vocab::EventKind, node: NodeKey, steps: usize) {
    if !on() {
        return;
    }
    let at = micros();
    eprintln!(
        "ZMT {at} anim.edge kind={kind:?} node={:?} steps={steps}",
        zgui_view_dom::id::to_view(node)
    );
}

/// Records an edge aimed at an element that has already left the tree.
pub(crate) fn gone(kind: zgui_vocab::EventKind, node: NodeKey) {
    let at = micros();
    eprintln!(
        "ZMT {at} anim.edge-gone kind={kind:?} node={:?}",
        zgui_view_dom::id::to_view(node)
    );
}

/// Records what the frame published as each element's running count.
pub(crate) fn published(counts: &rustc_hash::FxHashMap<NodeKey, usize>) {
    if !on() {
        return;
    }
    let at = micros();
    let mut listed: Vec<String> = counts
        .iter()
        .map(|(node, count)| format!("{:?}={count}", zgui_view_dom::id::to_view(*node)))
        .collect();
    listed.sort();
    eprintln!("ZMT {at} anim.running counts=[{}]", listed.join(","));
}
