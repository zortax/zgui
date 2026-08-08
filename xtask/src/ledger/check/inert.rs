//! Every enum variant something branches on is built by something.
//!
//! This is written against a shape of defect that has now appeared five times in this workspace and
//! was never once caught by a compiler, a test or a review: a case that is *defined*, *matched on*,
//! and *constructed nowhere*. A scrollbar fragment kind with a painter and a dump behind it and no
//! producer; a vector rasteriser nothing ever attached; a replayed operation range that was always
//! empty; a discrete scroll phase nothing ever reported; an animation-end listener bound to a node
//! that had already gone.
//!
//! Each of them reads perfectly. The type exists, the branch that handles it is written, tested and
//! correct, and the feature it implements is simply absent from the running program — which is the
//! one failure mode that looks identical to a feature that is merely unused. Nothing else here can
//! see it: the branch is live code, so no dead-code warning fires; the arm is covered by the type's
//! own tests, so no coverage gap opens; and the missing half is an *absence*, so there is nothing
//! for a reviewer to read.
//!
//! What this asserts is the one thing that is always true of the shape: a variant that is worth
//! branching on is worth building. So every variant named in a pattern must also be named in an
//! expression, somewhere in the workspace, and a variant that is never built is either a producer
//! nobody wrote or a branch nobody needs.
//!
//! # What it deliberately does not assert
//!
//! That every variant is *reached*. Reachability is a question about a running program and this is
//! a question about a source tree; a variant built only on a path no document takes is a different
//! defect and needs a different instrument. This one catches the case where the construction does
//! not exist at all, which is the case that has actually happened.

use std::collections::BTreeMap;

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;

/// Variants that are matched on and built by something this check cannot see.
///
/// Each row is an enum and one of its variants, with the reason the construction is invisible here.
/// A row is a claim that the producer exists somewhere a textual scan cannot reach it, not a
/// permission to leave one unwritten.
const ALLOWED: &[(&str, &str, &str)] = &[
    // Vocabulary an application hands *to* the framework. The framework branches on every value;
    // which one arrives is the program's choice, so the workspace legitimately builds none of them.
    (
        "CursorStyle",
        "Crosshair",
        "a cursor an application sets on a surface",
    ),
    (
        "CursorStyle",
        "Move",
        "a cursor an application sets on a surface",
    ),
    (
        "CursorStyle",
        "Progress",
        "a cursor an application sets on a surface",
    ),
    (
        "CursorStyle",
        "ResizeColumn",
        "a cursor an application sets on a surface",
    ),
    (
        "CursorStyle",
        "ResizeRow",
        "a cursor an application sets on a surface",
    ),
    (
        "CursorStyle",
        "VerticalText",
        "a cursor an application sets on a surface",
    ),
    (
        "CursorStyle",
        "Wait",
        "a cursor an application sets on a surface",
    ),
    (
        "FullscreenMode",
        "Exclusive",
        "a mode an application asks a surface for",
    ),
    (
        "FocusMove",
        "First",
        "a focus command an application issues",
    ),
    ("FocusMove", "Last", "a focus command an application issues"),
    ("FocusMove", "Next", "a focus command an application issues"),
    ("FocusMove", "Prev", "a focus command an application issues"),
    (
        "SidebarSide",
        "Right",
        "a component prop an application passes",
    ),
    (
        "SidebarCollapse",
        "Offcanvas",
        "a component prop an application passes",
    ),
    (
        "ToastCorner",
        "BottomLeft",
        "a component prop an application passes",
    ),
    (
        "ToastCorner",
        "TopLeft",
        "a component prop an application passes",
    ),
    (
        "ToastCorner",
        "TopRight",
        "a component prop an application passes",
    ),
    (
        "ScrollTarget",
        "By",
        "a destination an application asks a node to scroll to",
    ),
    (
        "ScrollTarget",
        "IntoViewCenter",
        "a destination an application asks a node to scroll to",
    ),
    (
        "ScrollTarget",
        "IntoViewEnd",
        "a destination an application asks a node to scroll to",
    ),
    (
        "SheetRequest",
        "Pending",
        "an answer an application's own sheet loader returns",
    ),
    (
        "ClipboardData",
        "Image",
        "what a platform backend that decodes images returns",
    ),
    (
        "Act",
        "Named",
        "a step a bench script names on the command line",
    ),
    // Classifications carried by data rather than by code. A register's rows say which of these
    // each property is, and a value nothing has yet been classified as is an unused label rather
    // than a missing producer.
    (
        "AbsentReason",
        "NeedsFork",
        "a classification no register row has needed yet",
    ),
    (
        "AbsentReason",
        "NotInLayout",
        "a classification no register row has needed yet",
    ),
    (
        "GapStatus",
        "NotYetImplemented",
        "a classification no register row has needed yet",
    ),
    // Built by a macro expansion, which is text this scan never sees.
    ("Either", "Right", "built by the view macro's own expansion"),
    // A declared absence. A style-uniform run within a line is what a rich-text editor needs and
    // what a document does not produce, and the painter says so where it draws one: until
    // something splits lines that far, a run is drawn through the path its line is.
    (
        "FragmentKind",
        "TextRun",
        "declared absent in `zgui-paint`'s emit order, with the reason",
    ),
];

/// Runs the check.
pub(crate) fn check(tree: &Tree) -> Report {
    let mut report = Report::clean();
    let mut variants = Variants::default();
    for member in &tree.members {
        for source in &member.sources {
            variants.declare(&source.rel_path, &source.text);
        }
    }
    for member in &tree.members {
        for source in &member.sources {
            variants.observe(&source.text);
        }
    }
    for (name, seen) in variants.rows() {
        report.violation(
            seen.declared_in.clone(),
            format!(
                "`{name}` is matched on in {} place(s) and constructed in none; either the \
                 producer was never written or the branch is not needed",
                seen.matched
            ),
        );
    }
    report
}

/// One variant, and what the workspace does with it.
#[derive(Debug, Default)]
struct Seen {
    /// The file the enum was declared in.
    declared_in: String,
    /// How many times it appears in a pattern.
    matched: usize,
    /// How many times it appears in an expression.
    built: usize,
}

/// Every variant declared in the tree, and what is done with each.
#[derive(Debug, Default)]
struct Variants {
    /// Keyed by `Enum::Variant`, which is how both halves name it.
    rows: BTreeMap<String, Seen>,
}

impl Variants {
    /// Records every variant one file declares.
    fn declare(&mut self, path: &str, text: &str) {
        for (enum_name, variant, defaulted) in declarations(text) {
            let row = self
                .rows
                .entry(format!("{enum_name}::{variant}"))
                .or_default();
            row.declared_in = path.to_owned();
            // A `#[default]` variant is built by the derived implementation, which is a producer
            // that exists and has no text naming it.
            if defaulted {
                row.built += 1;
            }
        }
    }

    /// Records every use one file makes of the variants declared anywhere in the tree.
    ///
    /// One pass over the lines and one map lookup per path in them, rather than a search for every
    /// known variant in every line: the second is the whole tree squared, and this runs over the
    /// whole workspace on every build of the gate.
    fn observe(&mut self, text: &str) {
        // Which enums this file declares, so that `Self::V` inside an impl is credited to the
        // enum it belongs to rather than to every enum in the workspace with a variant of that
        // name.
        let own: Vec<String> = declarations(text)
            .into_iter()
            .map(|(enum_name, _, _)| enum_name)
            .collect();
        for line in text.lines() {
            for (at, qualifier, variant) in paths_in(line) {
                let end = at + qualifier.len() + 2 + variant.len();
                let pattern = consumes(line, at, end);
                let candidates: Vec<String> = if qualifier == "Self" {
                    own.iter()
                        .map(|enum_name| format!("{enum_name}::{variant}"))
                        .collect()
                } else {
                    vec![format!("{qualifier}::{variant}")]
                };
                for key in candidates {
                    let Some(row) = self.rows.get_mut(&key) else {
                        continue;
                    };
                    if pattern {
                        row.matched += 1;
                    } else {
                        row.built += 1;
                    }
                }
            }
        }
    }

    /// Every variant that is matched on and never built.
    fn rows(&self) -> Vec<(&str, &Seen)> {
        self.rows
            .iter()
            .filter(|(key, seen)| {
                let (enum_name, variant) = key.split_once("::").expect("a qualified name");
                seen.matched > 0
                    && seen.built == 0
                    && !ALLOWED.iter().any(|(held, held_variant, _)| {
                        *held == enum_name && *held_variant == variant
                    })
            })
            .map(|(key, seen)| (key.as_str(), seen))
            .collect()
    }
}

/// Every `Qualifier::Variant` in one line, as its offset, its qualifier and its variant.
///
/// Only paths whose last two segments both begin with a capital letter, which is what an enum and
/// a variant look like and what a module path does not.
fn paths_in(line: &str) -> Vec<(usize, &str, &str)> {
    let bytes = line.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = line[at..].find("::") {
        let separator = at + offset;
        at = separator + 2;
        let start = word_start(bytes, separator);
        let end = word_end(bytes, at);
        if start == separator || end == at {
            continue;
        }
        let qualifier = &line[start..separator];
        let variant = &line[at..end];
        // A longer path — `crate::fragment::Kind::Thumb` — is named by its last two segments,
        // which are the only two that say which variant of which enum this is.
        let capitalised = |word: &str| word.starts_with(|c: char| c.is_ascii_uppercase());
        if capitalised(qualifier) && capitalised(variant) {
            found.push((start, qualifier, variant));
        }
        at = end;
    }
    found
}

/// Where the identifier ending at `end` begins.
fn word_start(bytes: &[u8], end: usize) -> usize {
    let mut start = end;
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    start
}

/// Where the identifier beginning at `start` ends.
fn word_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    end
}

/// The enum a line declares, if it declares one.
fn enum_header(line: &str) -> Option<&str> {
    let mut rest = line.trim_start();
    if let Some(after) = rest.strip_prefix("pub") {
        rest = match after.strip_prefix('(') {
            // `pub(crate) enum …`, `pub(super) enum …`
            Some(scope) => scope.split_once(')')?.1,
            None => after,
        }
        .trim_start();
    }
    let rest = rest.strip_prefix("enum ")?;
    let name = rest
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()?;
    (!name.is_empty()).then_some(name)
}

/// Every variant declared in `text`, as its enum, its name, and whether it is the derived default.
fn declarations(text: &str) -> Vec<(String, String, bool)> {
    let mut found = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let Some(name) = enum_header(lines[index]) else {
            index += 1;
            continue;
        };
        let depth = lines[index].len() - lines[index].trim_start().len();
        let name = name.to_owned();
        let mut defaulted = false;
        index += 1;
        while index < lines.len() {
            let line = lines[index];
            let trimmed = line.trim();
            if trimmed == "}" && line.len() - line.trim_start().len() == depth {
                break;
            }
            if trimmed.starts_with("#[default]") {
                defaulted = true;
                index += 1;
                continue;
            }
            if let Some(variant) = variant_name(trimmed) {
                found.push((name.clone(), variant.to_owned(), defaulted));
                defaulted = false;
            }
            index += 1;
        }
        index += 1;
    }
    found
}

/// The variant a line inside an enum body declares, if it declares one.
fn variant_name(trimmed: &str) -> Option<&str> {
    let first = trimmed.chars().next()?;
    if !first.is_ascii_uppercase() {
        return None;
    }
    let name = trimmed
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()?;
    let rest = trimmed[name.len()..].trim_start();
    let follows = rest.chars().next().unwrap_or(',');
    matches!(follows, ',' | '(' | '{' | '=').then_some(name)
}

/// Whether the occurrence between `start` and `end` is a pattern rather than an expression.
///
/// Textual, and deliberately biased towards calling an occurrence a construction: a pattern
/// mistaken for a construction silences this check for one variant, while a construction mistaken
/// for a pattern fails the build over a variant that is perfectly well built.
fn consumes(line: &str, start: usize, end: usize) -> bool {
    let before = &line[..start];
    let after = &line[end..];
    if line.contains("matches!(") || after.contains("=>") {
        return true;
    }
    // An occurrence to the right of the arrow is the arm's body, so it is a construction even where
    // the pattern it belongs to wrapped onto this line. rustfmt breaks an or-pattern that does not
    // fit — `A | B | C => Variant,` over three lines — and the line carrying the body then opens
    // with the `|` of the pattern above it. Read as a pattern, the variant is reported inert while
    // the line examined builds it, and no spelling of the arm satisfies both this check and
    // `cargo fmt`.
    if before.contains("=>") {
        return false;
    }
    let trimmed = before.trim_start();
    if trimmed.starts_with('|') || trimmed.is_empty() && after.trim_end().ends_with('|') {
        return true;
    }
    // `let E::V { … } = x`, `if let E::V(…) = x`, `let Some(E::V) = x else`: a binding whose
    // right-hand side comes after the path.
    before.contains("let ") && (after.contains(" = ") || after.contains("= "))
}

#[cfg(test)]
mod tests {
    use super::{consumes, declarations, enum_header, paths_in, variant_name};

    /// Whether the one occurrence of `path` in `line` is a pattern.
    fn is_pattern(line: &str, path: &str) -> bool {
        let at = line.find(path).expect("the path is in the line");
        consumes(line, at, at + path.len())
    }

    #[test]
    fn a_match_arm_is_a_pattern_and_a_return_is_a_construction() {
        assert!(is_pattern(
            "            Kind::Thumb => self.thumb,",
            "Kind::Thumb"
        ));
        assert!(!is_pattern(
            "        kinds.push(Kind::Thumb);",
            "Kind::Thumb"
        ));
        assert!(is_pattern(
            "    let Kind::Thumb { axis } = kind else { return };",
            "Kind::Thumb"
        ));
        assert!(is_pattern(
            "    if matches!(fragment.kind, Kind::Thumb { .. }) {",
            "Kind::Thumb"
        ));
    }

    #[test]
    fn the_body_of_a_wrapped_or_pattern_is_still_a_construction() {
        // The shape rustfmt produces from an or-pattern that does not fit: the arm's body sits on
        // a line that opens with the `|` of the pattern above it.
        assert!(!is_pattern(
            "            | Raw::Third => Kind::Thumb,",
            "Kind::Thumb"
        ));
        // The pattern on the same line is still a pattern.
        assert!(is_pattern(
            "            | Raw::Third => Kind::Thumb,",
            "Raw::Third"
        ));
    }

    #[test]
    fn a_path_is_read_by_its_last_two_capitalised_segments() {
        assert_eq!(
            paths_in("            crate::fragment::Kind::Thumb => 0,"),
            vec![(29, "Kind", "Thumb")],
            "the module path in front of the enum is not part of its name"
        );
        assert_eq!(
            paths_in("Kind::Thumbnail => 0,"),
            vec![(0, "Kind", "Thumbnail")],
            "and a longer variant is not a shorter one"
        );
        assert!(
            paths_in("        self.store.get(key)").is_empty(),
            "a method call is not a variant"
        );
    }

    #[test]
    fn an_enums_variants_are_read_out_of_its_body_and_nothing_elses() {
        let text = "\
/// What a fragment draws.
pub enum Kind {
    /// A box.
    Box,
    /// A bar.
    Scrollbar {
        /// Which axis.
        axis: Axis,
    },
}

pub enum Other {
    #[default]
    None,
}
";
        let found = declarations(text);
        assert_eq!(
            found,
            vec![
                ("Kind".to_owned(), "Box".to_owned(), false),
                ("Kind".to_owned(), "Scrollbar".to_owned(), false),
                ("Other".to_owned(), "None".to_owned(), true),
            ],
            "the fields of a struct variant are not variants, and a default is marked"
        );
        assert_eq!(enum_header("pub(crate) enum Bar {"), Some("Bar"));
        assert_eq!(variant_name("axis: Axis,"), None);
    }
}
