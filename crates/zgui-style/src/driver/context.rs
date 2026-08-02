//! Everything the workers of one traversal share.

use style::animation::DocumentAnimationSet;
use style::context::SharedStyleContext;
use style::global_style_data::GLOBAL_STYLE_DATA;
use style::selector_parser::SnapshotMap;
use style::shared_lock::StylesheetGuards;
use style::stylist::Stylist;
use style::traversal_flags::TraversalFlags;

use crate::driver::animations::AnimationTime;
use crate::driver::traversal::NoPainters;

/// Builds the context one traversal runs under.
///
/// Everything in it is read by several workers at once and written by none, which is why it is
/// assembled here in one place and handed over whole.
///
/// `flags` decides which traversal this is. Two of them exist and they are not interchangeable:
/// the ordinary one, which matches selectors and cascades, and the animation-only one, which
/// replaces an element's animation and transition declarations and touches nothing else. An
/// element whose animation asked for a cascade is only ever looked at by the second, and the first
/// refuses to process what the second was supposed to — so building every context as "ordinary"
/// leaves every animation that cannot be composed as a repaint permanently unserviced.
pub(crate) fn build<'a>(
    stylist: &'a Stylist,
    guards: StylesheetGuards<'a>,
    snapshots: &'a SnapshotMap,
    animations: DocumentAnimationSet,
    now: AnimationTime,
    flags: TraversalFlags,
) -> SharedStyleContext<'a> {
    SharedStyleContext {
        traversal_flags: flags,
        stylist,
        options: GLOBAL_STYLE_DATA.options.clone(),
        guards,
        // Visited-link styling is a document-language feature with a privacy history, and there is
        // no document language here to have visited links.
        visited_styles_enabled: false,
        animations,
        current_time_for_animations: now.seconds(),
        snapshot_map: snapshots,
        registered_speculative_painters: &NoPainters,
    }
}
