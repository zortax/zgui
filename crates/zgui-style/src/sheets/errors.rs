//! What the parser dropped, and where it was.
//!
//! Installing a stylesheet cannot fail, because the parser has no whole-sheet failure state: an
//! unrecognised declaration drops that declaration, a rejected selector drops that rule, an
//! at-rule this build does not implement drops that block, and everything else in the sheet
//! installs and applies. A fallible install would therefore have exactly one implementable
//! meaning — *any* complaint rejects the whole file — and one unknown property would delete every
//! rule beside it.
//!
//! So nothing is refused and everything is reported. A dropped item is otherwise completely
//! silent: it is not an error a caller gets back, it is a declaration or a rule that quietly does
//! not exist, and this sink is the only place either becomes visible.

use std::sync::Mutex;

use cssparser::SourceLocation;
use style::error_reporting::{ContextualParseError, ParseErrorReporter};
use style::stylesheets::UrlExtraData;

/// What kind of thing the parser dropped.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[non_exhaustive]
pub enum DropKind {
    /// One declaration, because the property or its value is not recognised. The rule around it
    /// keeps every other declaration.
    Declaration,
    /// One whole rule, because its selector was rejected. Nothing in the rule applies, so a rule
    /// dropped here is not merely unmatched — it does not exist.
    Rule,
    /// One at-rule block, because this build does not implement it.
    AtRule,
    /// Something the parser complained about that is none of the above, kept rather than
    /// discarded so that a new kind of complaint is visible before it has a category.
    Other,
}

/// One thing the parser dropped, with the place in the source it was dropped at.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct CssDiagnostic {
    /// Where in the sheet it was, counting lines and columns from zero.
    pub location: SourceLocation,
    /// What kind of thing was dropped.
    pub kind: DropKind,
    /// The parser's own description of it.
    pub message: String,
}

/// Everything one sheet's parse dropped.
///
/// Deliberately not `#[must_use]`: the common call site installs a sheet it wrote itself and
/// ignores the result, and every entry has already been logged by the time it is returned.
pub type CssDiagnostics = Vec<CssDiagnostic>;

/// Collects what the parser drops while one sheet is being parsed.
///
/// Wired in release as well as in debug. It runs once per sheet, at parse time, so it costs
/// nothing per frame — and a build in which a component library's sheet silently loses a rule is
/// the build a user runs.
#[derive(Debug, Default)]
pub(crate) struct DiagnosticSink {
    /// What has been reported so far.
    entries: Mutex<CssDiagnostics>,
}

impl DiagnosticSink {
    /// A sink with nothing in it.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Records something this crate dropped that the parser itself had no complaint about.
    ///
    /// An `@import` the loader refuses is the case: the parser is perfectly happy with the rule,
    /// and the sheet it names simply never arrives, which would otherwise be the quietest failure
    /// in the whole install path.
    pub(crate) fn record(&self, location: SourceLocation, kind: DropKind, message: String) {
        tracing::warn!(
            line = location.line,
            column = location.column,
            ?kind,
            "{message}"
        );
        self.entries
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .push(CssDiagnostic {
                location,
                kind,
                message,
            });
    }

    /// Everything reported so far, leaving the sink empty.
    pub(crate) fn take(&self) -> CssDiagnostics {
        core::mem::take(&mut self.entries.lock().unwrap_or_else(|held| held.into_inner()))
    }
}

impl ParseErrorReporter for DiagnosticSink {
    fn report_error(
        &self,
        url: &UrlExtraData,
        location: SourceLocation,
        error: ContextualParseError<'_>,
    ) {
        let kind = classify(&error);
        let message = error.to_string();
        tracing::warn!(
            sheet = %url.0,
            line = location.line,
            column = location.column,
            ?kind,
            "{message}"
        );
        self.entries
            .lock()
            .unwrap_or_else(|held| held.into_inner())
            .push(CssDiagnostic {
                location,
                kind,
                message,
            });
    }
}

/// Which category one of the parser's complaints falls into.
///
/// Classified by what was lost rather than by why, because that is what a caller can act on: a
/// dropped declaration leaves the rest of its rule working, a dropped rule leaves nothing of
/// itself, and a dropped at-rule is a feature this build does not have.
fn classify(error: &ContextualParseError<'_>) -> DropKind {
    match error {
        ContextualParseError::UnsupportedPropertyDeclaration(..)
        | ContextualParseError::UnsupportedPropertyDescriptor(..)
        | ContextualParseError::UnsupportedFontFaceDescriptor(..)
        | ContextualParseError::UnsupportedFontFeatureValuesDescriptor(..)
        | ContextualParseError::UnsupportedFontPaletteValuesDescriptor(..)
        | ContextualParseError::UnsupportedCounterStyleDescriptorDeclaration(..)
        | ContextualParseError::UnsupportedViewportDescriptorDeclaration(..)
        | ContextualParseError::UnsupportedViewTransitionDescriptor(..)
        | ContextualParseError::UnsupportedValue(..) => DropKind::Declaration,
        // An at-rule this build does not implement is reported through the *invalid rule* arm
        // rather than through the unsupported-rule one, with the offending source in hand — so
        // the source is what tells the two apart, and a category that read only the variant would
        // report `@container` as a rejected selector.
        ContextualParseError::InvalidRule(css, _) if css.trim_start().starts_with('@') => {
            DropKind::AtRule
        }
        ContextualParseError::InvalidRule(..)
        | ContextualParseError::InvalidKeyframeRule(..)
        | ContextualParseError::InvalidFontFeatureValuesRule(..)
        | ContextualParseError::InvalidMediaRule(..)
        | ContextualParseError::NeverMatchingHostSelector(..) => DropKind::Rule,
        ContextualParseError::UnsupportedRule(..) => DropKind::AtRule,
        _ => DropKind::Other,
    }
}
