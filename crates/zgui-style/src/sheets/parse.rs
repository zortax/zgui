//! Turning stylesheet source into a sheet the rule set can hold.

use std::str::FromStr;

use cssparser::SourceLocation;
use selectors::matching::QuirksMode;
use servo_arc::Arc as ServoArc;
use style::media_queries::MediaList;
use style::shared_lock::{Locked, SharedRwLock};
use style::stylesheets::import_rule::{
    ImportLayer, ImportRule, ImportSheet, ImportSupportsCondition,
};
use style::stylesheets::{
    AllowImportRules, DocumentStyleSheet, Origin, Stylesheet, StylesheetLoader, UrlExtraData,
};
use style::values::CssUrl;
use zgui_dom::{SheetLoader, SheetRequest};

use crate::sheets::errors::{DiagnosticSink, DropKind};

/// Everything one parse needs that is the same for every sheet in a document.
pub(crate) struct ParseContext<'a> {
    /// The lock the document, its sheets and every restyle guard share.
    pub(crate) lock: &'a SharedRwLock,
    /// The base the sheet's relative references are resolved against.
    pub(crate) url: &'a UrlExtraData,
    /// Where the source of a referenced sheet comes from.
    pub(crate) loader: &'a dyn SheetLoader,
    /// Where what the parser drops is recorded.
    pub(crate) sink: &'a DiagnosticSink,
}

impl ParseContext<'_> {
    /// Parses one sheet at `origin`.
    ///
    /// Never fails. Every dropped declaration, rule and at-rule reaches the sink with the place it
    /// was dropped at, and everything else in the sheet installs.
    pub(crate) fn parse(&self, css: &str, origin: Origin) -> DocumentStyleSheet {
        let media = ServoArc::new(self.lock.wrap(MediaList::empty()));
        DocumentStyleSheet(ServoArc::new(self.parse_inner(css, origin, media)))
    }

    /// The same, under the media list an `@import` attaches to the sheet it names.
    fn parse_inner(
        &self,
        css: &str,
        origin: Origin,
        media: ServoArc<Locked<MediaList>>,
    ) -> Stylesheet {
        Stylesheet::from_str(
            css,
            self.url.clone(),
            origin,
            media,
            self.lock.clone(),
            Some(&ImportBridge {
                context: self,
                origin,
            }),
            Some(self.sink),
            QuirksMode::NoQuirks,
            AllowImportRules::Yes,
        )
    }
}

/// Answers the parser's `@import` requests out of the document's installed loader.
///
/// The engine asks for the imported sheet *during* the parse and splices the answer into the
/// position the `@import` occupies, because an imported rule set cannot be inserted into the
/// middle of a sheet afterwards. A loader with nothing to say answers that the source is not
/// available and the import contributes nothing; the default loader refuses everything, so a
/// document that has installed none reports each `@import` as a dropped rule.
struct ImportBridge<'a> {
    /// What the imported sheet is parsed with.
    context: &'a ParseContext<'a>,
    /// The origin the importing sheet is at, which an imported sheet shares.
    origin: Origin,
}

impl StylesheetLoader for ImportBridge<'_> {
    fn request_stylesheet(
        &self,
        url: CssUrl,
        location: SourceLocation,
        lock: &SharedRwLock,
        media: ServoArc<Locked<MediaList>>,
        supports: Option<ImportSupportsCondition>,
        layer: ImportLayer,
    ) -> ServoArc<Locked<ImportRule>> {
        let base = self.context.url.0.as_str();
        let stylesheet = match self.context.loader.load(base, url.as_str()) {
            SheetRequest::Ready(source) => {
                let sheet = self.context.parse_inner(&source, self.origin, media);
                ImportSheet::new(ServoArc::new(sheet))
            }
            SheetRequest::Pending => {
                self.context.sink.record(
                    location,
                    DropKind::AtRule,
                    format!("the import of `{}` is not available yet", url.as_str()),
                );
                ImportSheet::new_pending()
            }
            SheetRequest::Rejected => {
                self.context.sink.record(
                    location,
                    DropKind::AtRule,
                    format!("the import of `{}` was refused by the loader", url.as_str()),
                );
                ImportSheet::new_refused()
            }
            // A loader answering in a way this build does not know about is a loader written
            // against a later version. Refusing the import is the conservative reading: the
            // reference is reported rather than silently contributing nothing.
            _ => {
                self.context.sink.record(
                    location,
                    DropKind::AtRule,
                    format!(
                        "the loader answered the import of `{}` in a way this build does not \
                         understand",
                        url.as_str()
                    ),
                );
                ImportSheet::new_refused()
            }
        };
        ServoArc::new(lock.wrap(ImportRule {
            url,
            stylesheet,
            supports,
            layer,
            source_location: location,
        }))
    }
}

/// The base every sheet with no URL of its own is parsed against.
///
/// Built by inference rather than by naming a URL library: the type is fixed by the field it goes
/// into, so the parse target never has to be written down and no crate above this one inherits a
/// dependency on it.
pub(crate) fn base_url() -> UrlExtraData {
    fn parsed<T: FromStr>(text: &str) -> T
    where
        T::Err: core::fmt::Debug,
    {
        text.parse().expect("a well-formed base URL")
    }
    UrlExtraData(ServoArc::new(parsed("zgui:///")))
}
