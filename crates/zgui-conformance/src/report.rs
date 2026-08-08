//! The measurement, written out as the document that is committed and reviewed.
//!
//! Everything here is derived: the denominator from the engine's generated property list, the
//! classification from declarations that live beside their readers, the evidence column from
//! probes that were run, the unreachable set from the engine's own definitions and the suite rates
//! from a conversion that happened during this run. Nothing is typed in, so the document cannot
//! disagree with the code.
//!
//! It is also byte-deterministic — no timestamp, no path, no iteration order that a hash decides.
//! That is what lets it be committed and compared: the check is that regenerating produces the file
//! that is already there, and a date stamped into it would make every run a diff and every diff
//! meaningless. When each row changed is the version history's answer, not the document's.

use core::fmt::Write as _;
use std::path::PathBuf;

use zgui_css::parity::{GAPS, Support};

use crate::census::Census;
use crate::evidence::{Survey, Verdict, unproven};
use crate::stanza::Definitions;
use crate::wpt::SuiteResult;

/// Where the generated document lives.
pub fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/parity.md")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/parity.md"))
}

/// Writes the whole measurement.
pub fn render(
    census: &Census,
    survey: &Survey,
    definitions: &Definitions,
    suites: &[SuiteResult],
) -> String {
    let mut out = String::new();
    preamble(&mut out);
    totals(&mut out, census, survey, definitions);
    suite_rates(&mut out, suites);
    disagreements(&mut out, census);
    unproven_rows(&mut out);
    gaps(&mut out);
    properties(&mut out, census, survey);
    out
}

/// The heading and what the document is.
fn preamble(out: &mut String) {
    out.push_str(
        "# CSS parity\n\
         \n\
         Generated. Every number here is measured by the conformance harness and none of it is\n\
         written by hand; regenerating it is part of the test suite, so a change to what the\n\
         framework supports arrives as a diff to this file.\n\
         \n\
         A property counts as **implemented** when some module declares that it reads the value,\n\
         and that declaration is only believed when setting the property on a fixture visibly\n\
         changes one of four things: the fragment tree, the answer hit testing gives, the shapes\n\
         boxes are clipped to, or what a style lowers to for painting. A declaration with no such\n\
         consequence fails the build unless it is listed, with a reason, under *claimed without\n\
         observable consequence* below.\n\
         \n\
         Parity here means parity with **what the style engine and the vector stack actually\n\
         support**. A feature that would need a patched or vendored build of the engine is out of\n\
         scope by decision — there is to be no fork — and is recorded under *out of reach* with\n\
         what an application should write instead. Everything else that is missing is work, and is\n\
         recorded separately.\n",
    );
}

/// The counts.
fn totals(out: &mut String, census: &Census, survey: &Survey, definitions: &Definitions) {
    let total = census.canonical().len();
    let proven = survey.proven().len();
    let out_of_reach = GAPS
        .iter()
        .filter(|gap| gap.status.is_out_of_reach())
        .count();
    let _ = write!(
        out,
        "\n## The numbers\n\
         \n\
         Three answers, kept apart: what is **implemented**, what is **not yet implemented** — \
         reachable\n\
         with the engine as it stands and simply not done — and what is **out of reach**, meaning \
         no\n\
         build of this framework can do it without a patched style engine. The last is a boundary \
         and\n\
         not a backlog, so it is never added to what is left to do.\n\
         \n\
         | | Count |\n|---|---:|\n\
         | Property names the engine generates | {} |\n\
         | Distinct longhands behind them | {total} |\n\
         | Classified | {} |\n\
         | Implemented | {} |\n\
         | Parsed and cascaded, not yet implemented | {} |\n\
         | Classified as unavailable from the engine | {} |\n\
         | Shown by probe to change what a frame produces | {proven} |\n\
         | Out of reach: defined by the engine for another target only | {} |\n\
         | Out of reach: register rows | {out_of_reach} |\n\
         | Not yet implemented: register rows | {} |\n",
        census.names().len(),
        census.classified(),
        census.implemented(),
        census.ignored(),
        census.absent(),
        definitions.other_engine_only().len(),
        GAPS.len() - out_of_reach,
    );
}

/// The converted suites and how much of each passes.
fn suite_rates(out: &mut String, suites: &[SuiteResult]) {
    out.push_str("\n## Converted reference suites\n\n| Suite | Passing | Tests | Unconvertible |\n|---|---:|---:|---:|\n");
    for suite in suites {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} |",
            suite.name, suite.passing, suite.tests, suite.unconvertible,
        );
    }
    out.push_str(
        "\nEach test is compared against its reference as a fragment tree rather than as pixels,\n\
         and the counts above are a floor that may never fall.\n",
    );
}

/// Where two crates answered differently about one property.
fn disagreements(out: &mut String, census: &Census) {
    if census.disagreements().is_empty() {
        return;
    }
    out.push_str(
        "\n## Declared twice, differently\n\
         \n\
         Declarations live beside the code that reads a property, so two crates can each answer for\n\
         their own reasons. The stronger answer is the one counted, and both are shown.\n\
         \n| Property | Counted | Also declared |\n|---|---|---|\n",
    );
    for row in census.disagreements() {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} |",
            row.css_name,
            describe(row.kept),
            describe(row.dropped),
        );
    }
}

/// The rows whose consequence this build cannot observe.
fn unproven_rows(out: &mut String) {
    out.push_str(
        "\n## Claimed without observable consequence\n\
         \n\
         These properties reach a consumer that this harness cannot exercise. The deterministic\n\
         shaper has one face and applies no feature by design, because a suite written against real\n\
         faces measures the machine it runs on.\n\
         \n| Property | Why no probe settles it |\n|---|---|\n",
    );
    for (css_name, reason) in unproven::ROWS {
        let _ = writeln!(out, "| `{css_name}` | {reason} |");
    }
}

/// The register: what this build cannot do, split by whether it ever will.
fn gaps(out: &mut String) {
    out.push_str(
        "\n## Out of reach\n\
         \n\
         Parity is parity with what the style engine and the vector stack support. These rows would\n\
         need a patched or vendored build of the engine, and there is to be none — so they are the\n\
         accepted boundary of this framework rather than work that is outstanding. Each says what\n\
         an application should write instead, and carries a probe, so a row that has quietly become\n\
         untrue fails.\n\
         \n| Missing | Why the stack cannot reach it | What to do instead | Standing |\n\
         |---|---|---|---|\n",
    );
    for gap in GAPS.iter().filter(|gap| gap.status.is_out_of_reach()) {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} |",
            gap.subject,
            gap.reason,
            gap.instead,
            gap.status.label(),
        );
    }

    out.push_str(
        "\n## Not yet implemented\n\
         \n\
         Reachable with the engine exactly as it stands, and therefore work rather than boundary.\n\
         \n| Missing | Why it is missing | What would close it | Owner |\n|---|---|---|---|\n",
    );
    let mut any = false;
    for gap in GAPS.iter().filter(|gap| !gap.status.is_out_of_reach()) {
        any = true;
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} |",
            gap.subject, gap.reason, gap.patch, gap.owner,
        );
    }
    if !any {
        out.push_str("| — | — | — | — |\n");
    }

    out.push_str(
        "\nWhat it would take to close a row that is out of reach is recorded too, in\n\
         `zgui-css::parity::Gap::patch`, so that a future engine release can be measured against\n\
         it. It is not a plan.\n",
    );
}

/// Every longhand, its answer and its evidence.
fn properties(out: &mut String, census: &Census, survey: &Survey) {
    out.push_str(
        "\n## Every longhand\n\n| Property | Treatment | Evidence | Where |\n|---|---|---|---|\n",
    );
    for css_name in census.canonical() {
        let Some(support) = census.answer(css_name) else {
            continue;
        };
        let evidence = match survey.verdict(css_name) {
            Some(Verdict::Changed) => "changes what a frame produces",
            Some(Verdict::Unchanged) => "no observable change",
            Some(Verdict::Inert) => "the probe reached no computed style",
            None => "not probed",
        };
        let _ = writeln!(
            out,
            "| `{css_name}` | {} | {evidence} | {} |",
            describe(support),
            support.note(),
        );
    }
}

/// One treatment in a word.
fn describe(support: Support) -> &'static str {
    match support {
        Support::Implemented(_) => "implemented",
        Support::Ignored(_) => "unread",
        Support::Absent(_) => "unavailable",
        // A treatment this build does not know the name of is still not one it consumes, and
        // saying so is what keeps the fraction honest rather than optimistic.
        _ => "unread",
    }
}

#[cfg(test)]
mod tests {
    use crate::census::Census;
    use crate::evidence::Survey;
    use crate::stanza::Definitions;

    use super::{path, render};

    /// The committed document is what the harness produces right now.
    ///
    /// Regenerating is part of the suite rather than a chore someone remembers, so a change to what
    /// the framework supports cannot land without the number that describes it landing too.
    #[test]
    fn the_committed_document_is_the_one_the_harness_produces() {
        let suites = crate::wpt::suite::run_all().expect("the corpus is readable");
        let definitions = Definitions::load().expect("the engine's definitions are readable");
        let rendered = render(&Census::take(), Survey::take(), &definitions, &suites);

        assert!(rendered.contains("| Implemented |"));
        assert!(
            rendered.lines().count() > 250,
            "{}",
            rendered.lines().count()
        );

        if crate::ratchet::is_blessing() {
            std::fs::write(path(), &rendered).expect("the document is writable");
            panic!(
                "rewrote {}; re-run without the blessing variable to check it",
                path().display()
            );
        }
        let committed = std::fs::read_to_string(path()).unwrap_or_default();
        assert_eq!(
            committed, rendered,
            "docs/parity.md is out of date; re-run with the blessing variable set",
        );
    }

    /// The same run renders the same bytes, which is what makes committing it useful.
    #[test]
    fn the_document_is_deterministic() {
        let suites = crate::wpt::suite::run_all().expect("the corpus is readable");
        let definitions = Definitions::load().expect("the engine's definitions are readable");
        let first = render(&Census::take(), Survey::take(), &definitions, &suites);
        let second = render(&Census::take(), Survey::take(), &definitions, &suites);
        assert_eq!(first, second);
    }
}
