//! The arithmetic that keeps the unclaimed-longhand list honest.
//!
//! The list of longhands nothing reads lives beside the property catalogue, because it has no
//! reader to sit beside. This is where it is checked, because this is the first crate that can see
//! every consumer at once.

use std::collections::BTreeSet;

/// No row in the unclaimed list names a property some crate has claimed a reader for.
///
/// Without this the list would be a description of nothing: a registry merges two answers for one
/// property rather than refusing them, so a row left behind when its reader landed would stay in
/// the unclaimed list, counted as unread, with the parity number understating the framework by
/// however many rows had been forgotten. The other net — a probe contradicting an `Ignored` row —
/// only catches a property whose effect this harness can see, which the whole painting group is
/// not.
#[test]
fn no_unclaimed_row_is_also_declared_beside_a_reader() {
    zgui_css::enable_css_features();
    let claimed: BTreeSet<String> = [
        zgui_style::parity::REGISTERED.to_vec(),
        zgui_text_style::parity::REGISTERED.to_vec(),
        zgui_layout::parity::registered(),
        zgui_css::parity::gap::inherited_svg::REGISTERED.to_vec(),
    ]
    .concat()
    .iter()
    .map(zgui_css::parity::Registration::css_name)
    .collect();

    let overlapping: Vec<String> = zgui_css::parity::backlog::registered()
        .iter()
        .map(zgui_css::parity::Registration::css_name)
        .filter(|css_name| claimed.contains(css_name))
        .collect();
    assert_eq!(
        overlapping,
        Vec::<String>::new(),
        "these are declared both in the unclaimed list and beside a reader; the reader's row is \
         the true one",
    );
    assert!(!claimed.is_empty(), "the claimed set was read as empty");
}
