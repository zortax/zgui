//! Where libxkbcommon's own diagnostics go.
//!
//! libxkbcommon writes to standard error by default, and `XKB_LOG_LEVEL` in the environment turns
//! it up. On a bare console standard error is often the very terminal a caller is drawing on, so a
//! keymap that fails to compile would corrupt the screen. Every [`crate::Context`] therefore takes
//! the messages away from the library the moment it is made.
//!
//! Nothing is printed unless a caller asks for it. [`crate::Context::set_log_sink`] is how a caller
//! asks, and it is also how the reason a keymap refused to compile reaches a log: that reason is
//! libxkbcommon's to give, and this is the only way it is given.
//!
//! # The thread a sink belongs to
//!
//! A message arrives on the thread that called into libxkbcommon, and a [`crate::Context`] never
//! leaves the thread that made it, so the sink is a property of the thread. Two contexts on one
//! thread share one sink. A caller that wants them apart keeps them on separate threads, which is
//! where they have to be anyway.

use std::cell::RefCell;
use std::ffi::{VaList, c_char, c_uint};

use crate::library::XkbContext;

// `vsnprintf` is the one C symbol this crate names at link time, and it comes out of the C library
// the standard library already links. libxkbcommon hands its messages over as a format string and
// a `va_list`, exactly as `vprintf` takes them, so there is nothing else to format them with.
unsafe extern "C" {
    /// Writes at most `size` bytes of the formatted text, and answers the length it needed.
    fn vsnprintf(buffer: *mut c_char, size: usize, format: *const c_char, args: VaList<'_>) -> i32;
}

/// Where libxkbcommon's messages go once a caller has said.
///
/// Called with each message on the thread that produced it, while the call that produced it is
/// still running. A sink that calls back into libxkbcommon loses whatever that call has to say.
pub type Sink = Box<dyn FnMut(LogLevel, &str)>;

/// The callback `xkb_context_set_log_fn` takes.
pub(crate) type LogFn = unsafe extern "C" fn(*mut XkbContext, c_uint, *const c_char, VaList<'_>);

/// How serious a message from libxkbcommon is.
///
/// The numbers are the library's own, and a level it grows later reads as [`LogLevel::Debug`],
/// which keeps the message rather than dropping it.
///
/// ```
/// use zgui_xkb::LogLevel;
///
/// assert!(LogLevel::Critical < LogLevel::Error);
/// assert!(LogLevel::Error < LogLevel::Debug);
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// An internal error the library cannot carry on through.
    Critical,
    /// Something the caller asked for did not work.
    Error,
    /// Something worked and is likely wrong.
    Warning,
    /// What the library is doing.
    Info,
    /// Everything.
    Debug,
}

impl LogLevel {
    /// Reads the number `xkb_log_level` holds.
    pub(crate) fn from_raw(raw: c_uint) -> Self {
        match raw {
            0..=10 => Self::Critical,
            11..=20 => Self::Error,
            21..=30 => Self::Warning,
            31..=40 => Self::Info,
            _ => Self::Debug,
        }
    }

    /// Returns the number `xkb_log_level` holds.
    pub(crate) fn raw(self) -> c_uint {
        match self {
            Self::Critical => 10,
            Self::Error => 20,
            Self::Warning => 30,
            Self::Info => 40,
            Self::Debug => 50,
        }
    }
}

/// The longest message this crate formats.
///
/// libxkbcommon's own default handler writes straight to a stream and has no limit. A message
/// longer than this is cut, which costs the end of a sentence in a log line.
const MESSAGE_LIMIT: usize = 1024;

thread_local! {
    /// Where this thread's messages go, once a caller has said.
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };

    /// Messages held for the call that is running, when one asked to keep them.
    static CAPTURED: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// Sends every message on this thread to `sink`, in place of whatever was there.
pub(crate) fn set_sink(sink: Option<Sink>) {
    SINK.with(|held| *held.borrow_mut() = sink);
}

/// Runs `call`, and answers with it and whatever libxkbcommon said while it ran.
///
/// This is how [`crate::Error::Keymap`] carries a reason. libxkbcommon reports why a keymap
/// refused to compile through its log and nowhere else: the call itself answers with nothing at
/// all, so without this the reason is lost.
pub(crate) fn capturing<T>(call: impl FnOnce() -> T) -> (T, String) {
    // A nested capture would take the messages from the outer one, and no call in this crate
    // nests. Arming over an armed buffer is therefore treated as the outer one owning it.
    let nested = CAPTURED.with(|held| held.borrow().is_some());
    if !nested {
        CAPTURED.with(|held| *held.borrow_mut() = Some(Vec::new()));
    }

    let answer = call();

    if nested {
        return (answer, String::new());
    }
    let lines = CAPTURED
        .with(|held| held.borrow_mut().take())
        .unwrap_or_default();
    (answer, lines.join("; "))
}

/// What libxkbcommon calls with every message it would have printed.
///
/// # Safety
///
/// This is called by libxkbcommon and by nothing else. `format` is a C string it owns, and `args`
/// are the arguments that go with it. The function is `extern "C"`, so a panic inside it aborts
/// rather than unwinding into C.
pub(crate) unsafe extern "C" fn deliver(
    _context: *mut XkbContext,
    level: c_uint,
    format: *const c_char,
    args: VaList<'_>,
) {
    if format.is_null() {
        return;
    }
    let mut buffer = [0_u8; MESSAGE_LIMIT];
    // SAFETY: `vsnprintf` writes at most `MESSAGE_LIMIT` bytes into the buffer and terminates
    // what it writes. `format` and `args` are the pair libxkbcommon passed, which is the pair
    // `vprintf` takes. The list is read once and never again.
    let written = unsafe { vsnprintf(buffer.as_mut_ptr().cast(), buffer.len(), format, args) };
    let Ok(written) = usize::try_from(written) else {
        return;
    };
    let text = String::from_utf8_lossy(&buffer[..written.min(buffer.len() - 1)]);
    let text = text.trim_end_matches('\n');
    if text.is_empty() {
        return;
    }

    let level = LogLevel::from_raw(level);
    // A sink that called back into libxkbcommon would find the cell already borrowed. Dropping
    // the message keeps that from being a panic inside a C call frame.
    let _ = CAPTURED.try_with(|held| {
        if let Ok(mut held) = held.try_borrow_mut()
            && let Some(lines) = held.as_mut()
        {
            lines.push(text.to_owned());
        }
    });
    let _ = SINK.try_with(|held| {
        if let Ok(mut held) = held.try_borrow_mut()
            && let Some(sink) = held.as_mut()
        {
            sink(level, text);
        }
    });
}

#[cfg(test)]
mod tests {
    //! The levels and the capture, over no library at all.

    use super::*;

    #[test]
    fn the_five_levels_read_as_the_library_numbers_them() {
        assert_eq!(LogLevel::from_raw(10), LogLevel::Critical);
        assert_eq!(LogLevel::from_raw(20), LogLevel::Error);
        assert_eq!(LogLevel::from_raw(30), LogLevel::Warning);
        assert_eq!(LogLevel::from_raw(40), LogLevel::Info);
        assert_eq!(LogLevel::from_raw(50), LogLevel::Debug);
        for level in [
            LogLevel::Critical,
            LogLevel::Error,
            LogLevel::Warning,
            LogLevel::Info,
            LogLevel::Debug,
        ] {
            assert_eq!(LogLevel::from_raw(level.raw()), level);
        }
    }

    #[test]
    fn a_level_this_crate_does_not_know_is_kept_rather_than_dropped() {
        // A libxkbcommon with a sixth level would otherwise lose whatever it said. `Debug` is the
        // answer that keeps the message.
        assert_eq!(LogLevel::from_raw(99), LogLevel::Debug);
    }

    #[test]
    fn a_call_that_says_nothing_captures_nothing() {
        let (answer, said) = capturing(|| 7);

        assert_eq!(answer, 7);
        assert!(said.is_empty());
    }

    #[test]
    fn what_was_said_during_a_call_comes_back_with_it() {
        let (answer, said) = capturing(|| {
            CAPTURED.with(|held| {
                if let Some(lines) = held.borrow_mut().as_mut() {
                    lines.push("no such layout".to_owned());
                    lines.push("failed to compile".to_owned());
                }
            });
            "refused"
        });

        assert_eq!(answer, "refused");
        assert_eq!(said, "no such layout; failed to compile");
    }

    #[test]
    fn the_buffer_is_put_back_after_a_call() {
        // A buffer left armed would collect every later message for the life of the thread, and
        // the next keymap error would carry the last one's reason.
        let (_, _) = capturing(|| ());

        assert!(CAPTURED.with(|held| held.borrow().is_none()));
    }
}
