//! The tails gate: no measurement may report only its middle.
//!
//! A median is what a run felt like on its good frames, and smoothness is decided by the other
//! ones. A 235 µs median with a 20 ms stall every thirtieth frame feels exactly as bad as it is and
//! passes every band written against a median — which is how this project came to hold a green
//! ratchet over a document that lost half its frame rate in sixty seconds.
//!
//! So the rule is mechanical and it is checked against the run the ratchet just stored rather than
//! against the source: every duration carries p50, p95, p99, max and the size of the population
//! they came from, and every scenario publishes how many of its frames were late against the
//! interval it drove them at. The gate reads the newest file under `docs/perf/runs/`, which means
//! it is checking the numbers a reader will actually see rather than a promise about them.

mod check;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// Where the ratchet stores each run.
const RUNS: &str = "docs/perf/runs";

/// Checks the newest stored run.
pub(crate) fn run(root: &Path) -> Result<()> {
    let runs = root.join(RUNS);
    let Some(newest) = newest(&runs) else {
        return Err(Error::failed(format!(
            "no run is stored under `{RUNS}`, so there is nothing to check the distributions of. \
             The ratchet stores one every time it runs, and this gate runs after it."
        )));
    };
    let text = std::fs::read_to_string(&newest).map_err(|error| {
        Error::failed(format!(
            "the stored run `{}` cannot be read: {error}",
            newest.display()
        ))
    })?;
    println!("checking {}", newest.display());

    let violations = check::violations(&text);
    if violations.is_empty() {
        println!("tails ok: every duration published a distribution and every scenario its pacing");
        return Ok(());
    }
    for violation in &violations {
        eprintln!("TAILS {} {}", violation.subject, violation.reason);
    }
    Err(Error::failed(format!(
        "{} measurements in `{}` report a middle and no tail",
        violations.len(),
        newest.display()
    )))
}

/// The most recently stored run, by the stamp its name is.
fn newest(runs: &Path) -> Option<PathBuf> {
    let mut stored: Vec<PathBuf> = std::fs::read_dir(runs)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "tsv"))
        .collect();
    stored.sort();
    stored.pop()
}
