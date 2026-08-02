//! The environment override that forces every frame to redraw the whole surface.

use std::env;
use std::sync::OnceLock;

/// The variable that switches the override on.
const VARIABLE: &str = "ZGUI_FULL_DAMAGE";

/// Whether `ZGUI_FULL_DAMAGE=1` is set in the environment.
///
/// When it is, [`DamageSet::for_frame`](crate::DamageSet::for_frame) starts every frame with the
/// whole surface damaged, which turns off partial redrawing wholesale. It exists so that "is this
/// artefact a damage-tracking bug?" is one restart away from an answer, and it is the first thing
/// to try when a visual artefact is reported.
///
/// The environment is read once, the first time this is called, and the answer is remembered.
pub fn full_damage_forced() -> bool {
    static FORCED: OnceLock<bool> = OnceLock::new();
    *FORCED.get_or_init(|| env::var(VARIABLE).is_ok_and(|value| value == "1"))
}
