//! Whether the checks that cost something are switched on.
//!
//! Some of what a display list promises cannot be asserted for free — a primitive naming a
//! coordinate system that is still the one it was pushed under costs a word of storage per
//! primitive and a lookup per primitive to find out. That is the wrong price for every frame of
//! every window and the right price for a run being reduced, so it is asked for by setting
//! `ZGUI_INVARIANTS` in the environment.
//!
//! One reader for the whole workspace. Two would be two spellings of the same switch, and the
//! failure they produce is a check that is on in one crate and off in another for a run somebody
//! believes is checked throughout.

use std::sync::OnceLock;

/// Whether the checks that cost something are switched on.
///
/// Read once: an application that turns them on does so for its whole run, and re-reading the
/// environment per frame would put a lookup in the frame loop for an answer that cannot change.
///
/// ```
/// // Whatever the environment says, asking twice gives the same answer.
/// assert_eq!(zgui_scene::invariant::enabled(), zgui_scene::invariant::enabled());
/// ```
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("ZGUI_INVARIANTS").is_ok_and(|value| value != "0" && !value.is_empty())
    })
}
