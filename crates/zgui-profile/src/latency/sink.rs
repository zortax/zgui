//! Where a mark goes, and how the recording reaches a file.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// One recorded moment.
struct Mark {
    /// What happened.
    stage: &'static str,
    /// A detail: which event, which outcome, how many of something.
    note: String,
    /// When, as nanoseconds since the recording's own zero.
    at_ns: u128,
}

/// The recording, once one has been started.
struct Recording {
    /// Where the file goes.
    path: String,
    /// The zero every mark is relative to.
    base: Instant,
    /// What that zero was in wall-clock terms, so a driver in another process can line up with it.
    base_unix_ns: u128,
    /// What has been recorded and not yet written.
    marks: Mutex<Vec<Mark>>,
}

/// Whether anything is being recorded, read on every mark.
static ENABLED: AtomicBool = AtomicBool::new(false);

/// The recording itself.
static RECORDING: OnceLock<Recording> = OnceLock::new();

/// Starts a recording if `ZGUI_LATENCY` names a file, and returns the moment it started.
///
/// Calling it a second time changes nothing: the zero of a recording is the first call's.
pub fn start_epoch() {
    let Ok(path) = std::env::var("ZGUI_LATENCY") else {
        return;
    };
    crate::latency::elements::read_environment();
    RECORDING.get_or_init(|| {
        let base_unix_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        ENABLED.store(true, Ordering::Relaxed);
        Recording {
            path,
            base: Instant::now(),
            base_unix_ns,
            marks: Mutex::new(Vec::with_capacity(1 << 16)),
        }
    });
}

/// Whether anything at all is listening: a file, a ring, or both.
///
/// One check in front of every mark, so the cost of instrumentation in a build nobody is measuring
/// stays at a relaxed load whichever of the two sinks exists.
fn listening() -> bool {
    ENABLED.load(Ordering::Relaxed) || crate::latency::ring::retaining()
}

/// Records that `stage` happened now, with nothing further to say about it.
pub fn mark(stage: &'static str) {
    if !listening() {
        return;
    }
    record(stage, String::new(), Instant::now());
}

/// Records that `stage` happened now, with `note` describing it.
pub fn note(stage: &'static str, note: impl Into<String>) {
    if !listening() {
        return;
    }
    record(stage, note.into(), Instant::now());
}

/// Records that `stage` happened now, with `describe` called only if anything is recording.
///
/// The form to reach for whenever the description costs an allocation or a traversal. A mark whose
/// note is built before the enabled check is paid for on every frame of every run, recording or
/// not, which makes the instrumentation part of what it is measuring.
pub fn note_with(stage: &'static str, describe: impl FnOnce() -> String) {
    if !listening() {
        return;
    }
    record(stage, describe(), Instant::now());
}

/// Records that `stage` happened at `at`, which a caller that already read the clock supplies.
pub fn mark_at(stage: &'static str, at: Instant) {
    if !listening() {
        return;
    }
    record(stage, String::new(), at);
}

/// Reads the clock once and returns something that marks with it, or `None` when nothing records.
///
/// For a caller that wants the moment before it takes a lock rather than after.
pub fn marker() -> Option<Instant> {
    listening().then(std::time::Instant::now)
}

/// Appends a mark to whichever sinks are listening.
fn record(stage: &'static str, note: String, at: Instant) {
    crate::latency::ring::push(stage, &note, at);
    let Some(recording) = RECORDING.get() else {
        return;
    };
    let at_ns = at.saturating_duration_since(recording.base).as_nanos();
    if let Ok(mut marks) = recording.marks.lock() {
        marks.push(Mark { stage, note, at_ns });
    }
}

/// Writes everything recorded so far, and empties what is held.
///
/// Called often enough that a run killed from outside still leaves a usable trace.
pub fn flush() {
    let Some(recording) = RECORDING.get() else {
        return;
    };
    let taken = {
        let Ok(mut marks) = recording.marks.lock() else {
            return;
        };
        if marks.is_empty() {
            return;
        }
        core::mem::take(&mut *marks)
    };
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&recording.path)
    else {
        return;
    };
    let mut out = std::io::BufWriter::new(file);
    let mut escaped = String::new();
    for mark in taken {
        escape_into(&mut escaped, &mark.note);
        let _ = writeln!(
            out,
            r#"{{"t":{},"base_unix_ns":{},"stage":"{}","note":"{}"}}"#,
            mark.at_ns, recording.base_unix_ns, mark.stage, escaped
        );
    }
    let _ = out.flush();
}

/// Rewrites `note` into `into` so that it can stand inside a JSON string.
///
/// A note is whatever the caller had to say, and callers say things like the debug rendering
/// of a rectangle — which contains quotation marks. Interpolating one of those into a JSON
/// string produces a line no reader can parse, and a reader that skips unparseable lines
/// silently drops exactly the marks that carry the most detail: the damage sets, the sizes,
/// the counts. Every measurement taken from such a file is short by however many of its
/// richest records happened to contain a quote.
fn escape_into(into: &mut String, note: &str) {
    into.clear();
    for character in note.chars() {
        match character {
            '"' => into.push_str("\\\""),
            '\\' => into.push_str("\\\\"),
            '\n' => into.push_str("\\n"),
            '\r' => into.push_str("\\r"),
            '\t' => into.push_str("\\t"),
            // A control character is not legal unescaped inside a JSON string, and a note is
            // arbitrary text: anything below the space is written as its own escape.
            c if (c as u32) < 0x20 => {
                use core::fmt::Write as _;
                let _ = write!(into, "\\u{:04x}", c as u32);
            }
            c => into.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::escape_into;

    /// Whether `escaped` could stand between the quotes of a JSON string.
    ///
    /// A quote or a control character may appear only as part of an escape, and a backslash
    /// may only introduce one. Written out rather than delegated to a parser because this
    /// crate carries no JSON dependency and must not grow one to check its own output.
    fn is_a_json_string_body(escaped: &str) -> bool {
        let mut rest = escaped.chars();
        while let Some(character) = rest.next() {
            match character {
                '"' => return false,
                c if (c as u32) < 0x20 => return false,
                '\\' => match rest.next() {
                    Some('"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't') => {}
                    Some('u') => {
                        if !(0..4).all(|_| rest.next().is_some_and(|d| d.is_ascii_hexdigit())) {
                            return false;
                        }
                    }
                    _ => return false,
                },
                _ => {}
            }
        }
        true
    }

    #[test]
    fn a_note_that_contains_a_quote_still_leaves_a_line_a_reader_can_parse() {
        // The regression: `d.postlayout` writes the debug rendering of a damage rectangle,
        // which carries `space: "device"`. Unescaped, that closed the JSON string early and
        // every damage mark in the trace was dropped by the reader as malformed — which is
        // silent, because a reader that skips bad lines still produces a plausible number.
        let mut out = String::new();
        escape_into(&mut out, r#"rects=[Rect { space: "device" }]"#);
        assert!(is_a_json_string_body(&out), "unescapable note: {out}");
        assert_eq!(out, r#"rects=[Rect { space: \"device\" }]"#);
    }

    #[test]
    fn a_backslash_a_newline_and_a_control_character_all_survive() {
        let mut out = String::new();
        escape_into(&mut out, "a\\b\nc\td\u{1}e");
        assert!(is_a_json_string_body(&out), "unescapable note: {out}");
        assert_eq!(out, "a\\\\b\\nc\\td\\u0001e");
    }

    #[test]
    fn an_ordinary_note_is_left_exactly_as_it_was() {
        // The negative control. Escaping that also rewrote ordinary notes would change every
        // number in every trace ever taken, and the two tests above would not notice.
        let note = "device=3840x2160 scale=1.2 mhz=Some(240000) interval_us=4166";
        let mut out = String::new();
        escape_into(&mut out, note);
        assert_eq!(out, note);
    }

    #[test]
    fn the_checker_rejects_what_the_bug_produced() {
        // Without this the three tests above are satisfied by a checker that says yes to
        // everything, including the unescaped note that started all of it.
        assert!(!is_a_json_string_body(r#"space: "device""#));
        assert!(!is_a_json_string_body("a\nb"));
        assert!(!is_a_json_string_body("trailing\\"));
    }

    #[test]
    fn the_buffer_is_not_appended_to_across_notes() {
        // `flush` reuses one buffer for every mark in the batch. Forgetting to clear it makes
        // each line carry every note before it, which no reader would reject.
        let mut out = String::new();
        escape_into(&mut out, "first");
        escape_into(&mut out, "second");
        assert_eq!(out, "second");
    }
}
