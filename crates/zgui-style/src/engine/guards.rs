//! The read guard every stylesheet and every restyle is taken under.
//!
//! Stylesheets, `style` attributes and the engine's own restyle state all live behind one lock,
//! and it is the *document's*: a declaration block behind a different lock cannot be read under
//! the guard a restyle takes, so a rule set built before any document exists would have to hand
//! its lock to the document rather than the other way round.
//!
//! The engine asks for two guards — one for author-origin sheets and one for the user-agent and
//! user origins — because an embedder may keep the two behind different locks. This one does not,
//! so both are the same guard, and saying so in one place is what stops half the calls taking one
//! lock and half the other.

use style::shared_lock::{SharedRwLock, SharedRwLockReadGuard, StylesheetGuards};

/// Runs `body` under a read guard on `lock`, with both of the engine's guards pointing at it.
pub(crate) fn with_guards<R>(
    lock: &SharedRwLock,
    body: impl FnOnce(&StylesheetGuards<'_>) -> R,
) -> R {
    let read = lock.read();
    body(&guards(&read))
}

/// Both of the engine's guards, over one read guard.
pub(crate) fn guards<'a>(read: &'a SharedRwLockReadGuard<'a>) -> StylesheetGuards<'a> {
    StylesheetGuards {
        author: read,
        ua_or_user: read,
    }
}
