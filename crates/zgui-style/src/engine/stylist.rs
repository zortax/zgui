//! The rule set, and the one thing about it that has to be decided after a traversal rather than
//! before one.
//!
//! `rem`, `rlh`, `rex`, `rch` and `ric` all resolve against the *root element's computed* values —
//! which are only known once the root has been styled. So the root's style is pushed back into the
//! device at the tail of a restyle, and if it moved while something had already resolved a unit
//! against the previous value, the traversal runs once more. Without that, `rem` is not stale, it
//! is wrong: every descendant sized in `rem` resolved against the initial font size rather than
//! against the one the sheet just set.

use selectors::matching::QuirksMode;
use style::device::Device;
use style::stylist::Stylist;
use zgui_dom::Document;

/// A rule set for `device`, with no stylesheets in it yet.
pub(crate) fn new(device: Device) -> Stylist {
    Stylist::new(device, QuirksMode::NoQuirks)
}

/// The root-relative quantities the cascade resolves units against.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub(crate) struct RootMetrics {
    /// The root element's computed font size, in CSS pixels.
    font_size: f32,
    /// The root element's computed line height, in CSS pixels.
    line_height: f32,
}

/// Pushes the root element's computed metrics into the device.
///
/// Reports whether the traversal has to run again, which is the case only when a metric *moved*
/// **and** something had already resolved a unit against the value it moved from. A document whose
/// root font size is whatever it always was, or one in which nothing is sized in `rem`, converges
/// in one pass.
pub(crate) fn push_root_metrics(
    stylist: &Stylist,
    document: &Document,
    last: &mut RootMetrics,
) -> bool {
    let Some(style) = document.root().and_then(|root| root.primary_style()) else {
        return false;
    };
    let device = stylist.device();

    let font_size = style.get_font().font_size.computed_size().px();
    let line_height = device
        .calc_line_height(style.get_font(), style.writing_mode, None)
        .0
        .px();

    device.set_root_style(&style);
    device.set_root_font_size(font_size);
    device.set_root_line_height(line_height);
    // Reads the root style just pushed and recomputes `ex`, `ch`, `cap` and `ic` against it.
    let metrics_moved = device.update_root_font_metrics();

    let moved = RootMetrics {
        font_size,
        line_height,
    };
    let was = core::mem::replace(last, moved);

    // Each half is guarded by whether anything actually read the quantity, which is what keeps the
    // fixpoint at one pass for a document that uses none of these units.
    (was.font_size != font_size && device.used_root_font_size())
        || (was.line_height != line_height && device.used_root_line_height())
        || (metrics_moved && device.used_root_font_metrics())
}
