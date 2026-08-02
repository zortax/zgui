//! Whether anything is recorded at all.

use std::cell::Cell;

thread_local! {
    /// The answer, read from the environment once and then remembered.
    static ON: Cell<Option<bool>> = const { Cell::new(None) };
}

/// The variable that turns the trace on.
const VARIABLE: &str = "ZGUI_MODAL_TRACE";

/// Whether trace lines are being written.
///
/// Off unless `ZGUI_MODAL_TRACE` is set in the environment, and read from there once: a build with
/// the variable unset pays one branch on an already-answered question per event, which is what
/// makes it safe to leave the calls in the components rather than in a copy of them.
pub(crate) fn on() -> bool {
    ON.with(|held| match held.get() {
        Some(answer) => answer,
        None => {
            let answer = std::env::var_os(VARIABLE).is_some();
            held.set(Some(answer));
            answer
        }
    })
}
