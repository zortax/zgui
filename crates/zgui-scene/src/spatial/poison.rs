//! A test mode that moves the document away from the device origin.
//!
//! Set `ZGUI_POISON_ORIGIN=<int>` to lay the document out at `(P, P)` and draw it through a
//! document space of `translate(-P, -P)`. The pixels stay the same. A reader that uses a fragment
//! rectangle as a device rectangle without putting it through [`Placements`](crate::Placements)
//! is then wrong by `P` pixels, and a pixel, hit or accessibility test finds it.
//!
//! The offset is an integer, so device-pixel snapping keeps whole-pixel edges whole. At the
//! offset an f32 holds less precision, so a fractional value can move by a few ulp, and a test
//! that compares such values exactly needs a tolerance of that size.

use std::sync::OnceLock;

/// The offset `ZGUI_POISON_ORIGIN` gives, or zero when it is unset or is not an integer.
///
/// Read once per process.
pub fn origin() -> f32 {
    static ORIGIN: OnceLock<f32> = OnceLock::new();
    *ORIGIN.get_or_init(|| {
        std::env::var("ZGUI_POISON_ORIGIN")
            .ok()
            .and_then(|raw| raw.trim().parse::<i32>().ok())
            .map_or(0.0, |offset| offset as f32)
    })
}
