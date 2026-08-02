//! What the run found, one line per claim.
//!
//! A claim carries the observation it was made from, not only its verdict, because a line that
//! says only `ok` is a line nobody can check. Every failure names what was expected and what was
//! there instead, in the same field, so the file can be read without the script beside it.

use std::fmt::Write as _;
use std::path::PathBuf;

/// How a claim came out.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// It did what it should.
    Works,
    /// It did something else.
    Broken,
    /// It could not be driven far enough to say.
    Unknown,
}

impl Verdict {
    /// The word this verdict is written as.
    const fn word(self) -> &'static str {
        match self {
            Self::Works => "works",
            Self::Broken => "BROKEN",
            Self::Unknown => "unknown",
        }
    }
}

/// One claim about one component.
struct Claim {
    /// Which component.
    component: String,
    /// What was being checked.
    about: String,
    /// How it came out.
    verdict: Verdict,
    /// What was seen.
    detail: String,
}

/// Everything the run found.
pub(crate) struct Report {
    /// The claims, in the order they were made.
    claims: Vec<Claim>,
    /// Where the report is written.
    path: PathBuf,
}

impl Report {
    /// A report that will be written to `path`.
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            claims: Vec::new(),
            path,
        }
    }

    /// Records a claim.
    pub(crate) fn claim(&mut self, component: &str, about: &str, verdict: Verdict, detail: &str) {
        println!(
            "{:<7} {:<22} {:<34} {detail}",
            verdict.word(),
            component,
            about
        );
        self.claims.push(Claim {
            component: component.to_owned(),
            about: about.to_owned(),
            verdict,
            detail: detail.to_owned(),
        });
    }

    /// Records a claim that holds when `passed`.
    pub(crate) fn check(&mut self, component: &str, about: &str, passed: bool, detail: &str) {
        let verdict = if passed {
            Verdict::Works
        } else {
            Verdict::Broken
        };
        self.claim(component, about, verdict, detail);
    }

    /// Records something that is neither a pass nor a failure.
    pub(crate) fn note(&mut self, component: &str, detail: &str) {
        self.claim(component, "note", Verdict::Unknown, detail);
    }

    /// Records a rectangle of the window, in device pixels, under `name`.
    ///
    /// A picture is only evidence about a component if the part of it that was looked at is the
    /// part that component drew, so the rectangle comes from the laid-out document and is written
    /// down beside the capture rather than chosen afterwards by eye.
    pub(crate) fn rect(&mut self, name: &str, x: f32, y: f32, width: f32, height: f32) {
        self.claim(
            "rect",
            name,
            Verdict::Unknown,
            &format!("{x:.1} {y:.1} {width:.1} {height:.1}"),
        );
    }

    /// How many claims came out broken.
    pub(crate) fn broken(&self) -> usize {
        self.claims
            .iter()
            .filter(|claim| claim.verdict == Verdict::Broken)
            .count()
    }

    /// Writes the report out.
    ///
    /// # Errors
    ///
    /// Returns whatever stopped the file being written.
    pub(crate) fn write(&self) -> std::io::Result<()> {
        let mut text = String::new();
        for claim in &self.claims {
            let _ = writeln!(
                text,
                "{}\t{}\t{}\t{}",
                claim.verdict.word(),
                claim.component,
                claim.about,
                claim.detail
            );
        }
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, text)
    }
}
