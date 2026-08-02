//! How wide the cascade may run.

use stylo_static_prefs::{pref, set_pref};

/// The most workers the cascade may be given, however many cores the machine has.
///
/// This is a hard ceiling of the engine's, not a tuning choice: the gain from more workers levels
/// off around here, and the engine's own sizing clamps to it. A pool built wider than this is not
/// merely wasteful — the extra workers are outside what the engine's per-worker storage is sized
/// for, so a pool of any width must be clamped to this before it is handed over.
pub const MAX_STYLE_THREADS: usize = 6;

/// Asks for a cascade `threads` workers wide, clamped to [`MAX_STYLE_THREADS`].
///
/// A width of zero or one means the cascade runs on the calling thread with no pool at all, which
/// is the right answer for a document small enough that starting workers costs more than the work
/// saved.
///
/// Takes effect only before the engine's shared worker pool is first used, and is ignored
/// afterwards, so it belongs beside the feature flags at start-up.
///
/// ```
/// use zgui_css::engine::threads::{MAX_STYLE_THREADS, request_style_threads, requested_style_threads};
///
/// request_style_threads(64);
/// assert_eq!(requested_style_threads(), Some(MAX_STYLE_THREADS));
/// ```
pub fn request_style_threads(threads: usize) {
    let clamped = threads.min(MAX_STYLE_THREADS);
    set_pref!(
        "layout.threads",
        i32::try_from(clamped).unwrap_or(MAX_STYLE_THREADS as i32)
    );
}

/// Lets the engine size the pool itself, up to [`MAX_STYLE_THREADS`].
pub fn autosize_style_threads() {
    set_pref!("layout.threads", -1i32);
}

/// The width asked for, or `None` when the engine is sizing the pool itself.
pub fn requested_style_threads() -> Option<usize> {
    let threads: i32 = pref!("layout.threads");
    usize::try_from(threads).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_STYLE_THREADS, autosize_style_threads, request_style_threads, requested_style_threads,
    };

    #[test]
    fn a_request_wider_than_the_ceiling_is_clamped_to_it() {
        request_style_threads(64);
        assert_eq!(requested_style_threads(), Some(MAX_STYLE_THREADS));

        request_style_threads(2);
        assert_eq!(requested_style_threads(), Some(2));

        autosize_style_threads();
        assert_eq!(requested_style_threads(), None);
    }
}
