//! What documentation may not say.
//!
//! Documentation that stands alone is documentation a reader can act on without the repository's
//! own working notes. A sentence that defers to a schedule, a numbered section or a review is
//! useless to that reader and becomes wrong the moment the note it points at is rewritten, with
//! nothing to notice the drift.
//!
//! Each rule below matches only the *referring* form, never the ordinary word. A frame has phases,
//! a reconciliation has a plan, and a glyph is rasterised at a subpixel phase; none of those are
//! matched.

use crate::docs::scan::{Cursor, contains_word, word_then_digit};

/// One thing documentation may not say, and the reason.
pub(crate) struct Rule {
    /// The rule's name, printed against a violation.
    pub(crate) name: &'static str,
    /// What to write instead, printed against a violation.
    pub(crate) instead: &'static str,
    /// Whether `line` breaks the rule.
    pub(crate) matches: fn(&str) -> bool,
}

/// Every rule, in the order they are reported.
pub(crate) const RULES: &[Rule] = &[
    Rule {
        name: "section-sign",
        instead: "name the specification and its chapter in words",
        matches: |line| line.contains('§'),
    },
    Rule {
        name: "planning-document",
        instead: "state the fact here, or link a rustdoc item that states it",
        matches: names_a_planning_document,
    },
    Rule {
        name: "numbered-phase",
        instead: "say what is true of the code, not when it arrived",
        matches: |line| word_then_digit(line, "phase") || word_then_digit(line, "phases"),
    },
    Rule {
        name: "scheduled-phase",
        instead: "describe what exists; a reader cannot act on what is scheduled",
        matches: defers_to_a_schedule,
    },
    Rule {
        name: "wave",
        instead: "say what is true of the code, not which group of work it belongs to",
        matches: names_a_wave,
    },
    Rule {
        name: "milestone",
        instead: "say what is true of the code, not which milestone contains it",
        matches: |line| contains_word(line, "milestone") || contains_word(line, "milestones"),
    },
    Rule {
        name: "proposal",
        instead: "document the decision, not the document that proposed it",
        matches: |line| contains_word(line, "proposal") || contains_word(line, "proposals"),
    },
    Rule {
        name: "review-round",
        instead: "state the conclusion; a reader has no access to the review",
        matches: names_a_review,
    },
    Rule {
        name: "review-reference",
        instead: "state the finding in words rather than by its identifier",
        matches: names_a_review_finding,
    },
    Rule {
        name: "spike",
        instead: "describe the code that shipped, not the experiment behind it",
        matches: names_a_spike,
    },
];

/// The repository's own working notes, by file name and by directory.
const PLANNING: [&str; 7] = [
    "PHASES.md",
    "PLAN.md",
    "ROADMAP.md",
    "CONSTRAINTS.md",
    "COMPOSITOR-PLAN.md",
    "COMPOSITOR-PHASES.md",
    "docs/planning",
];

/// Whether the line names one of the repository's working notes.
fn names_a_planning_document(line: &str) -> bool {
    PLANNING.iter().any(|name| line.contains(name))
}

/// The adjectives that turn "phase" from a stage of a frame into a place on a schedule.
const SCHEDULED: [&str; 6] = [
    "later",
    "earlier",
    "future",
    "subsequent",
    "upcoming",
    "forthcoming",
];

/// The verbs that do the same.
const SCHEDULING: [&str; 6] = ["will", "would", "lands", "landed", "ships", "shipped"];

/// Whether the line puts a phase on a schedule rather than in a frame.
fn defers_to_a_schedule(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    SCHEDULED
        .iter()
        .any(|word| lower.contains(&format!("{word} phase")))
        || SCHEDULING
            .iter()
            .any(|word| lower.contains(&format!("phase {word}")))
        || SCHEDULING
            .iter()
            .any(|word| lower.contains(&format!("phases {word}")))
}

/// Whether the line names a group of scheduled work, as in "wave Q".
fn names_a_wave(line: &str) -> bool {
    let mut cursor = Cursor::new(line);
    while let Some(rest) = cursor.after_word("wave") {
        let rest = rest.trim_start();
        let mut characters = rest.chars();
        // A single upper-case letter and nothing alphanumeric after it: "wave Q" names a group,
        // "wave function" and "wave Qx" do not.
        if let (Some(letter), next) = (characters.next(), characters.next())
            && letter.is_ascii_uppercase()
            && next.is_none_or(|next| !next.is_alphanumeric())
        {
            return true;
        }
    }
    false
}

/// Whether the line names a review of this repository.
fn names_a_review(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("review round") || word_then_digit(line, "round")
}

/// Whether the line names a review finding by its identifier, as in "R10".
///
/// A texture format such as `R8G8B8A8` is not one: the identifier has to stand as a whole word.
fn names_a_review_finding(line: &str) -> bool {
    let mut cursor = Cursor::new(line);
    while let Some((word, _)) = cursor.next_word() {
        let mut characters = word.chars();
        if characters.next() != Some('R') {
            continue;
        }
        let digits: String = characters.collect();
        if !digits.is_empty() && digits.len() <= 2 && digits.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    false
}

/// Whether the line names a risk-retirement experiment.
fn names_a_spike(line: &str) -> bool {
    contains_word(line, "spike") || contains_word(line, "spikes") || contains_word(line, "spiked")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every rule fires on the form it exists to catch.
    #[test]
    fn each_rule_fires_on_its_own_violation() {
        let planted = [
            ("section-sign", "/// See CSS 2.1 §10.8 for the strut."),
            (
                "planning-document",
                "/// The schedule is in docs/planning/PHASES.md.",
            ),
            ("numbered-phase", "/// Landed in phase 27."),
            ("scheduled-phase", "//! A later phase replaces this."),
            ("wave", "/// Part of wave Q."),
            ("milestone", "/// Due in the component-library milestone."),
            ("proposal", "/// Per the layering proposal."),
            ("review-round", "/// Raised in review round 4."),
            ("review-reference", "/// The R10 escape hatch."),
            ("spike", "/// The inline spike measured this."),
        ];
        for (name, line) in planted {
            let rule = RULES
                .iter()
                .find(|rule| rule.name == name)
                .expect("the rule exists");
            assert!((rule.matches)(line), "{name} did not fire on {line:?}");
        }
    }

    /// The ordinary words the framework's own documentation uses are not violations.
    #[test]
    fn the_domain_vocabulary_is_not_a_violation() {
        let innocent = [
            "/// The subpixel phase a glyph is rasterised at.",
            "/// The frame's last phase converts them into one request.",
            "/// The plan is built from the right, which is what makes it cheap.",
            "/// Rounds the advance to the nearest whole pixel.",
            "/// The R8G8B8A8 surface format.",
            "/// Capture, target and bubble are the three phases of a dispatch.",
        ];
        for line in innocent {
            for rule in RULES {
                assert!(
                    !(rule.matches)(line),
                    "{} fired on the innocent line {line:?}",
                    rule.name
                );
            }
        }
    }
}
