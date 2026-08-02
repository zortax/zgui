//! Installing stylesheets, keeping them in cascade order, and reporting what the parser dropped.
//!
//! | Module | Contents |
//! |---|---|
//! | [`origin`] | which of the three cascade origins a sheet belongs to |
//! | [`parse`] | source text to a parsed sheet, including what an `@import` names |
//! | [`set`] | which sheets are installed, in what order, and what a dropped handle means |
//! | [`replace`] | rewriting a sheet's text without moving it in the cascade |
//! | [`errors`] | what the parser dropped, and where |
//! | [`loader`] | where the source of a referenced sheet comes from |
//! | [`ua`] | this framework's own user-agent sheet |

pub mod errors;
pub mod loader;
pub mod origin;
pub mod parse;
pub mod replace;
pub mod set;
pub mod ua;

use style::shared_lock::SharedRwLock;
use style::stylesheets::UrlExtraData;
use style::stylist::Stylist;
use zgui_dom::SheetLoader;

use crate::sheets::errors::{CssDiagnostics, DiagnosticSink};
use crate::sheets::origin::SheetOrigin;
use crate::sheets::parse::ParseContext;
use crate::sheets::set::{SheetHandle, SheetSet};

/// Where one sheet's source comes from.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SheetSource<'a> {
    /// The text itself.
    Text(&'a str),
    /// A name the document's installed loader resolves.
    ///
    /// A name the loader refuses installs an empty sheet and reports it, which is the same
    /// outcome the parser gives an `@import` that cannot be resolved: the reference is visible in
    /// the diagnostics rather than being a rule set that silently lacks a file.
    Named(&'a str),
}

/// The stylesheets installed in one rule set.
pub(crate) struct Sheets {
    /// Which sheets are installed, and in what order.
    pub(crate) set: SheetSet,
    /// Where the parser's complaints are collected.
    pub(crate) sink: DiagnosticSink,
    /// The base relative references are resolved against.
    pub(crate) url: UrlExtraData,
}

impl Sheets {
    /// No sheets, and nothing reported.
    pub(crate) fn new() -> Self {
        Self {
            set: SheetSet::new(),
            sink: DiagnosticSink::new(),
            url: parse::base_url(),
        }
    }

    /// Parses `source` and appends it to `origin`'s sheets.
    pub(crate) fn add(
        &mut self,
        stylist: &mut Stylist,
        lock: &SharedRwLock,
        loader: &dyn SheetLoader,
        origin: SheetOrigin,
        source: SheetSource<'_>,
    ) -> (SheetHandle, CssDiagnostics) {
        let sheet = self.parse(lock, loader, origin, source);
        let guard = lock.read();
        let handle = self.set.append(stylist, &guard, origin, sheet);
        (handle, self.sink.take())
    }

    /// The same, placing the sheet immediately before the one `before` names.
    pub(crate) fn insert_before(
        &mut self,
        stylist: &mut Stylist,
        lock: &SharedRwLock,
        loader: &dyn SheetLoader,
        origin: SheetOrigin,
        source: SheetSource<'_>,
        before: &SheetHandle,
    ) -> (SheetHandle, CssDiagnostics) {
        let sheet = self.parse(lock, loader, origin, source);
        let guard = lock.read();
        let handle = self
            .set
            .insert_before(stylist, &guard, origin, sheet, before);
        (handle, self.sink.take())
    }

    /// Removes every sheet whose handle has been dropped, and reports how many went.
    pub(crate) fn remove_dropped(&mut self, stylist: &mut Stylist, lock: &SharedRwLock) -> usize {
        if !self.set.has_dropped() {
            return 0;
        }
        let guard = lock.read();
        self.set.remove_dropped(stylist, &guard)
    }

    /// Parses one sheet, resolving a named source through `loader`.
    pub(crate) fn parse(
        &self,
        lock: &SharedRwLock,
        loader: &dyn SheetLoader,
        origin: SheetOrigin,
        source: SheetSource<'_>,
    ) -> style::stylesheets::DocumentStyleSheet {
        let context = ParseContext {
            lock,
            url: &self.url,
            loader,
            sink: &self.sink,
        };
        let text = match source {
            SheetSource::Text(text) => std::borrow::Cow::Borrowed(text),
            SheetSource::Named(name) => match loader.load(self.url.0.as_str(), name) {
                zgui_dom::SheetRequest::Ready(text) => {
                    std::borrow::Cow::Owned(text.as_ref().to_owned())
                }
                other => {
                    tracing::warn!(
                        sheet = name,
                        answer = ?core::mem::discriminant(&other),
                        "the installed stylesheet loader did not supply a named sheet"
                    );
                    std::borrow::Cow::Borrowed("")
                }
            },
        };
        context.parse(&text, origin.to_engine())
    }
}
