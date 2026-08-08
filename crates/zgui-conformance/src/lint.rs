//! *"I wrote CSS and nothing happened"*, detected while it happens.
//!
//! Every other instrument here is static: it reads declarations, definitions and probes and says
//! what the framework supports. None of them can catch the thing an author actually experiences,
//! which is writing a property that this build parses, cascades and then does nothing with. That is
//! not a missing property — the value is right there on the computed style — and no register can
//! notice it, because the register is what already said the property is unread.
//!
//! So this walks a frame's computed styles and reports, once per property, one that carries a
//! non-initial value while being declared unread. Once per property rather than once per element,
//! because a table of a thousand rows would otherwise report the same mistake a thousand times.
//!
//! ```
//! use zgui_conformance::IgnoredLint;
//! use zgui_conformance::zdoc::{Zdoc, build::lay_out};
//!
//! let mut lint = IgnoredLint::new();
//!
//! let wrote_something_unread = Zdoc::parse(
//!     "@css\nroot { line-break: strict }\n@tree\nroot\n",
//! ).expect("well formed");
//! assert_eq!(lint.inspect(&lay_out(&wrote_something_unread).styles()), ["line-break"]);
//!
//! // And only once: the second frame says nothing about a property already reported.
//! assert!(lint.inspect(&lay_out(&wrote_something_unread).styles()).is_empty());
//! ```

use std::collections::BTreeSet;

use zgui_css::parity::{Registration, Support, observe};
use zgui_css::{ComputedStyle, StyleDraft};

/// Reports properties an author wrote that nothing reads.
pub struct IgnoredLint {
    /// The properties declared unread, with the note saying why.
    watched: Vec<(String, &'static str)>,
    /// A style with nothing but initial values, which is what "the author wrote something" means.
    initial: ComputedStyle,
    /// What has already been reported, so nothing is reported twice.
    reported: BTreeSet<String>,
}

impl IgnoredLint {
    /// A lint over every declaration in the workspace.
    pub fn new() -> Self {
        Self::over(&crate::registrations())
    }

    /// A lint over a particular set of declarations.
    pub fn over(rows: &[Registration]) -> Self {
        zgui_css::enable_css_features();
        Self {
            watched: rows
                .iter()
                .filter_map(|row| match row.support() {
                    Support::Ignored(note) => Some((row.css_name(), note)),
                    _ => None,
                })
                .collect(),
            initial: StyleDraft::initial().build(),
            reported: BTreeSet::new(),
        }
    }

    /// Reports the unread properties `styles` carry a value for, that have not been reported yet.
    ///
    /// The result is the *newly* reported ones, so a caller can log them and a test can assert on
    /// them. An empty result is the ordinary case and means every author-written property in this
    /// frame reaches something.
    pub fn inspect(&mut self, styles: &[ComputedStyle]) -> Vec<String> {
        let mut fresh = Vec::new();
        for (css_name, _) in &self.watched {
            if self.reported.contains(css_name) {
                continue;
            }
            let written = styles
                .iter()
                .any(|style| observe::differs_from_initial(style, &self.initial, css_name));
            if written {
                self.reported.insert(css_name.clone());
                fresh.push(css_name.clone());
            }
        }
        fresh
    }

    /// Why a reported property does nothing, for a message an author can act on.
    pub fn note(&self, css_name: &str) -> Option<&'static str> {
        self.watched
            .iter()
            .find(|(name, _)| name == css_name)
            .map(|(_, note)| *note)
    }

    /// Every property reported so far.
    pub fn reported(&self) -> impl Iterator<Item = &str> {
        self.reported.iter().map(String::as_str)
    }

    /// How many properties the lint is watching.
    pub fn watching(&self) -> usize {
        self.watched.len()
    }
}

impl Default for IgnoredLint {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::zdoc::Zdoc;
    use crate::zdoc::build::lay_out;

    use super::IgnoredLint;

    /// Lays out one sheet over a one-element document.
    fn styles(css: &str) -> Vec<zgui_css::ComputedStyle> {
        let source = format!("@css\nroot {{ {css} }}\n@tree\nroot \"text\"\n");
        lay_out(&Zdoc::parse(&source).expect("well formed")).styles()
    }

    /// The lint fires on a document that sets an unread property.
    #[test]
    fn it_fires_on_a_property_nothing_reads() {
        let mut lint = IgnoredLint::new();
        assert!(lint.watching() > 60, "{}", lint.watching());
        let fired = lint.inspect(&styles("table-layout: fixed"));
        assert_eq!(fired, ["table-layout"]);
        assert!(lint.note("table-layout").is_some());
    }

    /// And is silent on a document that sets only properties something reads.
    ///
    /// The control. A lint that fired on everything would be as useless as one that fired on
    /// nothing, and only running both cases tells them apart.
    #[test]
    fn it_is_silent_on_a_property_something_reads() {
        let mut lint = IgnoredLint::new();
        assert_eq!(
            lint.inspect(&styles("width: 120px; padding: 4px; font-size: 20px")),
            Vec::<String>::new(),
        );
        assert_eq!(lint.reported().count(), 0);
    }

    /// A property is reported once, however many frames it survives.
    #[test]
    fn it_reports_each_property_once() {
        let mut lint = IgnoredLint::new();
        let written = styles("table-layout: fixed; mask-mode: luminance");
        let first = lint.inspect(&written);
        assert_eq!(first.len(), 2, "{first:?}");
        assert_eq!(lint.inspect(&written), Vec::<String>::new());
        assert_eq!(lint.reported().count(), 2);
    }
}
