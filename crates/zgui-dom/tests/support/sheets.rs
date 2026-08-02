//! Parsing stylesheets, and hearing about what the parser dropped.
//!
//! Worth having even here: an unknown property is not an error the caller gets back, it is a
//! declaration that quietly does not exist, and a rejected selector is a whole rule that quietly
//! does not exist. This sink is the only place either is visible, and one of the cases is written
//! entirely against it.

use std::str::FromStr;
use std::sync::Mutex;

use cssparser::SourceLocation;
use selectors::matching::QuirksMode;
use servo_arc::Arc as ServoArc;
use style::error_reporting::{ContextualParseError, ParseErrorReporter};
use style::media_queries::MediaList;
use style::shared_lock::SharedRwLock;
use style::stylesheets::{AllowImportRules, DocumentStyleSheet, Origin, Stylesheet, UrlExtraData};

/// Everything the parser complained about.
#[derive(Debug, Default)]
pub(crate) struct Errors {
    /// The messages, in the order they were reported.
    messages: Mutex<Vec<String>>,
}

impl Errors {
    /// An empty sink.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Everything reported so far.
    pub(crate) fn messages(&self) -> Vec<String> {
        self.messages
            .lock()
            .expect("no panic while reporting")
            .clone()
    }
}

impl ParseErrorReporter for Errors {
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

/// The base URL every sheet here is parsed against.
///
/// Built by inference rather than by naming the URL library: the type is fixed by the field it goes
/// into, so the parse target never has to be written down.
pub(crate) fn base_url() -> UrlExtraData {
    fn parsed<T: FromStr>(text: &str) -> T
    where
        T::Err: std::fmt::Debug,
    {
        text.parse().expect("a well-formed base URL")
    }
    UrlExtraData(ServoArc::new(parsed("zgui:///tests")))
}

/// Parses one sheet, reporting whatever the parser dropped to `errors`.
pub(crate) fn parse(
    css: &str,
    origin: Origin,
    lock: &SharedRwLock,
    url: &UrlExtraData,
    errors: &Errors,
) -> DocumentStyleSheet {
    let media = ServoArc::new(lock.wrap(MediaList::empty()));
    DocumentStyleSheet(ServoArc::new(Stylesheet::from_str(
        css,
        url.clone(),
        origin,
        media,
        lock.clone(),
        None,
        Some(errors),
        QuirksMode::NoQuirks,
        AllowImportRules::No,
    )))
}

/// Whether a rule using `selector` survives parsing in this build.
///
/// A rejected selector invalidates the whole rule, which the parser reports, so the error sink
/// answers this directly — and it has to be asked separately from any matching test, because a test
/// written against a dropped rule passes while applying an empty sheet.
pub(crate) fn selector_parses(selector: &str) -> bool {
    let lock = SharedRwLock::new();
    let errors = Errors::new();
    parse(
        &format!("{selector} {{ color: rgb(1, 2, 3) }}"),
        Origin::Author,
        &lock,
        &base_url(),
        &errors,
    );
    errors.messages().is_empty()
}
