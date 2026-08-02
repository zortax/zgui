//! The surface a document is styled against, and what changing it invalidates.
//!
//! # Why this is a phase and not a setter
//!
//! Replacing the device is three separate invalidations wearing one name, and each of them reaches
//! the document by a different route. Getting any of them wrong is silent.
//!
//! * **Media queries.** Which rules apply can change, and nothing about that is recorded on any
//!   node. The rule set is told that the origins the change disturbed are dirty, and the restyle
//!   gate reads *that* rather than a node's obligations — a gate that only looked at nodes would
//!   be false and the whole mechanism inert.
//! * **Viewport units.** `50vw` resolves at computed-value time, so no amount of relaying out
//!   fixes a stale one; the elements that read a viewport unit have to cascade again. Whether any
//!   did is a flag on the device that answered the question, which is why it is read off the
//!   *outgoing* device: a replacement starts with the flag clear, so reading it afterwards always
//!   answers "no" and the branch never runs.
//! * **Geometry.** A size change relays the document out from the root; a pixel-ratio change
//!   relays every box out, because they are all snapped to a different grid.
//!
//! | Module | Contents |
//! |---|---|
//! | [`viewport`] | the surface's three quantities, and what each invalidates |
//! | [`color_scheme`] | light or dark |
//! | [`build`] | constructing the device itself |
//! | [`metrics`] | how the cascade asks a font system how tall an `ex` is |

pub mod build;
pub mod color_scheme;
pub mod metrics;
pub mod viewport;

use std::sync::Arc;

use style::shared_lock::SharedRwLock;
use style::stylist::Stylist;
use zgui_bits::Dirty;
use zgui_dom::Document;
use zgui_dom::dirty::propagate;
use zgui_text::FontMetricsSource;

pub use crate::device::color_scheme::ColorScheme;
pub use crate::device::metrics::MetricsAdapter;
pub use crate::device::viewport::Viewport;

use crate::engine::guards;
use crate::sheets::origin::OriginMask;

/// What replacing the device did.
///
/// Every field is an answer a later stage or a test asks for; nothing here is a summary of the
/// others. In particular an empty [`origins`](DeviceEpoch::origins) with `changed` set is the
/// ordinary case: a resize that crosses no media-query boundary disturbs no rules at all.
#[derive(Clone, Default, Debug)]
pub struct DeviceEpoch {
    /// Whether the surface moved at all, and therefore whether a device was built.
    pub changed: bool,
    /// The cascade origins whose media queries may now answer differently.
    pub origins: OriginMask,
    /// Whether the outgoing device had ever resolved a viewport unit.
    pub viewport_units: bool,
    /// How many elements were marked as needing to cascade again because they read one.
    pub units_invalidated: usize,
    /// How many elements were marked as needing to be laid out again.
    ///
    /// One for a size change, which marks the root and lets the layout cache decide how far down
    /// the change actually reaches; every element for a pixel-ratio change.
    pub relaid_out: usize,
}

/// Replaces the device with one built for `next`, and invalidates what that change invalidates.
///
/// Does nothing at all when the surface has not moved, which is every frame but the few that
/// follow a resize, a monitor change or a theme flip.
pub(crate) fn epoch(
    stylist: &mut Stylist,
    lock: &SharedRwLock,
    document: &mut Document,
    metrics: &Arc<dyn FontMetricsSource>,
    previous: Viewport,
    next: Viewport,
) -> DeviceEpoch {
    let change = previous.changes_to(next);
    if !change.any() {
        return DeviceEpoch::default();
    }

    // Read before the replacement, off the device that answered the questions. The device built
    // below starts with every "was this ever read" flag clear, so asking it instead always answers
    // no and the viewport-unit branch never runs at all.
    let viewport_units = stylist.device().used_viewport_size();

    let device = build::build(next, metrics);
    let origins = guards::with_guards(lock, |guards| stylist.set_device(device, guards));
    // Setting the device does not by itself re-match anything: the origins it reports are the ones
    // whose media queries may answer differently, and telling the rule set they are dirty is what
    // makes the next restyle re-collect their rules. An *empty* answer is the ordinary case — a
    // resize that crosses no query boundary — and it is passed on only when it is non-empty,
    // because forcing an empty set still marks the rule set as changed and would restyle the whole
    // document for a resize that disturbed no rule at all.
    if !origins.is_empty() {
        stylist.force_stylesheet_origins_dirty(origins);
    }

    let units_invalidated = if change.size && viewport_units {
        viewport::invalidate_units(document)
    } else {
        0
    };

    let relaid_out = if change.scale {
        viewport::relayout_everything(document)
    } else if change.size {
        match document.root_index() {
            Some(root) => {
                propagate::mark(document.store_mut(), root, Dirty::RELAYOUT);
                1
            }
            None => 0,
        }
    } else {
        0
    };

    DeviceEpoch {
        changed: true,
        origins: OriginMask::from_engine(origins),
        viewport_units,
        units_invalidated,
        relaid_out,
    }
}
