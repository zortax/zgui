//! Whether a scroll container draws its content in a coordinate system of its own.
//!
//! With the switch on, a scroll container gives its children a space that its scroll offset
//! moves, the first time it scrolls. A scroll frame then writes that space's matrix and leaves
//! every piece below the container as it is: their rectangles, clip chains and hit entries are
//! measured in the content space, which the scroll does not change. With the switch off, the
//! fragment pass offsets every piece below the container instead.
//!
//! Two kinds of box do not move with the content. A `position: fixed` box hangs from the space
//! above the content space, so the scroll does not move it. A sticky box is measured against the
//! port, so a scroll composes it again against the moved port. A scroll frame visits both, and the
//! path down to each, so the subtree inks above them stay true. A container keeps its children in
//! its own space when a fixed box below hangs from a transformed or sticky box below it, because
//! the content space would move that box.
//!
//! Set `ZGUI_SCROLL_NODES=1` to turn it on. [`set_enabled`] overrides the environment for the
//! process.

use std::sync::atomic::{AtomicU8, Ordering};

/// Not read yet.
const UNREAD: u8 = 0;
/// Off.
const OFF: u8 = 1;
/// On.
const ON: u8 = 2;

/// The answer, once read.
static STATE: AtomicU8 = AtomicU8::new(UNREAD);

/// Whether scroll containers draw their content in a space of their own.
///
/// Reads `ZGUI_SCROLL_NODES` on first use, unless [`set_enabled`] gave an answer before.
pub fn enabled() -> bool {
    match STATE.load(Ordering::Relaxed) {
        ON => true,
        OFF => false,
        _ => {
            let on = std::env::var("ZGUI_SCROLL_NODES").is_ok_and(|raw| raw.trim() == "1");
            let state = if on { ON } else { OFF };
            // A concurrent `set_enabled` wins over the environment.
            match STATE.compare_exchange(UNREAD, state, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => on,
                Err(held) => held == ON,
            }
        }
    }
}

/// Sets the answer for this process, over the environment.
///
/// For tests that need one answer whatever the environment says. The fragment pass reads the
/// answer once per walk, so a change takes effect at the next walk.
pub fn set_enabled(on: bool) {
    STATE.store(if on { ON } else { OFF }, Ordering::Relaxed);
}
