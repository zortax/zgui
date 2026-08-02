//! Asking the parser, rather than the matcher, whether a syntax exists in this build.
//!
//! A rejected selector is not a rule that fails to match — it is a rule that is not there, and
//! every declaration inside it goes with it. So a test written against such a syntax passes while
//! applying an empty style sheet, and reports success for a feature that does nothing. The only
//! way to tell the two apart is to parse a sheet and ask what the parser complained about, which
//! is what this does.

use servo_arc::Arc as ServoArc;
use std::str::FromStr;
use std::sync::Mutex;
use style::context::QuirksMode;
use style::error_reporting::{ContextualParseError, ParseErrorReporter};
use style::media_queries::MediaList;
use style::shared_lock::SharedRwLock;
use style::stylesheets::{AllowImportRules, Origin, Stylesheet, UrlExtraData};
use style::values::SourceLocation;

/// Whether a rule written with `selector` survives parsing in this build.
///
/// ```
/// use zgui_css::parity::selector_is_accepted;
///
/// assert!(selector_is_accepted("box:is(.card, .btn)"));
/// ```
pub fn selector_is_accepted(selector: &str) -> bool {
    complaints(&format!("{selector} {{ color: rgb(1, 2, 3) }}")).is_empty()
}

/// Everything the parser dropped while reading `css`, one message per drop.
///
/// Order is the order the parser reported them, and each message carries the line and column of
/// what was dropped — an empty result means the whole sheet was taken as written.
pub fn complaints(css: &str) -> Vec<String> {
    let sink = Sink::default();
    let lock = SharedRwLock::new();
    let media = ServoArc::new(lock.wrap(MediaList::empty()));
    let _ = Stylesheet::from_str(
        css,
        base_url(),
        Origin::Author,
        media,
        lock,
        None,
        Some(&sink),
        QuirksMode::NoQuirks,
        AllowImportRules::No,
    );
    sink.take()
}

/// The base URL every probe sheet is parsed against.
///
/// Built by inference rather than by naming a URL library: the type is fixed by the field it goes
/// into, so the parse target never has to be written down.
fn base_url() -> UrlExtraData {
    fn parsed<T: FromStr>(text: &str) -> T
    where
        T::Err: core::fmt::Debug,
    {
        text.parse().expect("a well-formed base URL")
    }
    UrlExtraData(ServoArc::new(parsed("zgui:///parity")))
}

/// Collects what the parser threw away.
#[derive(Default)]
struct Sink {
    /// The messages, in report order.
    messages: Mutex<Vec<String>>,
}

impl Sink {
    /// Everything reported so far.
    fn take(&self) -> Vec<String> {
        core::mem::take(&mut *self.messages.lock().expect("no panic while reporting"))
    }
}

impl ParseErrorReporter for Sink {
    fn report_error(
        &self,
        _url: &UrlExtraData,
        location: SourceLocation,
        error: ContextualParseError,
    ) {
        self.messages
            .lock()
            .expect("no panic while reporting")
            .push(format!("{}:{} {error:?}", location.line, location.column));
    }
}
