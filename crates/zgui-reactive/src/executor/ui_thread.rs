//! Which thread runs reactive work.

use std::cell::Cell;

thread_local! {
    /// Set once, by [`claim`], on a thread that has installed the reactive runtime.
    static CLAIMED: Cell<bool> = const { Cell::new(false) };
}

/// Whether the calling thread runs reactive work.
///
/// True on a thread that has successfully called
/// [`install`](crate::executor::install), false everywhere else. Reading and writing signals is
/// allowed from any thread; *running* tasks, cleaning up owners and dereferencing anything held
/// in a local context are not.
///
/// An application has exactly one such thread. Tests and tooling may install a runtime per
/// thread, and each one is then independent: its task pool, its current owner and its frame
/// waker belong to that thread alone.
///
/// Answers `false` on a thread being torn down, where the answer no longer means anything.
#[must_use]
pub fn is_ui_thread() -> bool {
    CLAIMED.try_with(Cell::get).unwrap_or(false)
}

/// Marks the calling thread as running reactive work.
pub(crate) fn claim() {
    CLAIMED.set(true);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thread_that_never_installed_is_not_the_ui_thread() {
        assert!(!is_ui_thread());
        std::thread::spawn(|| assert!(!is_ui_thread()))
            .join()
            .unwrap();
    }

    #[test]
    fn claiming_is_thread_local() {
        claim();
        assert!(is_ui_thread());
        std::thread::spawn(|| assert!(!is_ui_thread()))
            .join()
            .unwrap();
    }
}
