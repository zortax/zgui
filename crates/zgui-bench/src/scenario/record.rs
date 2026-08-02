//! The ratchet: run all five, store what they said, regenerate the document, fail on a regression.
//!
//! Three obligations, and they are separable on purpose.
//!
//! **Storing** happens whatever the verdict. A run that regressed is the run somebody most wants
//! the numbers from, so it is written to `docs/perf/runs/` before anything is compared.
//!
//! **Regenerating** `docs/performance.md` happens from the run that was just taken, so the document
//! in the tree is never a claim about a build nobody has: if the numbers moved, the document moved
//! with them and the change is in the diff.
//!
//! **Failing** happens last, and names every measurement that left its band rather than the first,
//! because a change that costs one thing usually costs several and a gate that stops at the first
//! turns one investigation into four.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// One parsed `MEASURE` line.
struct Row {
    /// Which scenario reported it.
    scenario: String,
    /// What was measured.
    name: String,
    /// What it is counted in.
    unit: String,
    /// What this run measured.
    value: f64,
    /// The largest value inside the band.
    limit: f64,
    /// Whether it stayed inside.
    passed: bool,
    /// Why the band is this wide.
    rationale: String,
    /// The schedule's budget for this quantity, where it wrote one, and whether the run met it.
    budget: Option<(f64, bool)>,
    /// The distribution the value was taken from, as the run spelled it, or `-` where there is
    /// none.
    spread: String,
}

/// One parsed `PACE` line.
struct Paced {
    /// Which scenario reported it.
    scenario: String,
    /// The interval its frames were driven at, in microseconds.
    interval_us: f64,
    /// How many of them missed it by half again.
    late: usize,
    /// How many there were.
    frames: usize,
}

/// One parsed `ESCALATION` line.
struct Escalated {
    /// Which scenario reported it.
    scenario: String,
    /// How many primitives reached the display list.
    emitted: u64,
    /// How many a clip refused.
    culled: u64,
    /// How many insertions the draw-order tree took.
    inserts: u64,
}

/// The repository this harness was built from.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate sits two levels under the workspace root")
        .to_path_buf()
}

/// Runs every scenario in a process of its own and returns what they printed.
fn sweep() -> String {
    let binary = std::env::current_exe().expect("the running binary can be named");
    let mut collected = String::new();
    for scenario in crate::scenario::ALL {
        println!("== scenario {scenario}");
        let output = std::process::Command::new(&binary)
            .args(["scenario", scenario])
            .output()
            .expect("the sweep can run this binary again");
        let text = String::from_utf8_lossy(&output.stdout);
        print!("{text}");
        assert!(
            output.status.success(),
            "scenario `{scenario}` did not finish: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr),
        );
        collected.push_str(&text);
    }
    collected
}

/// Pulls the rows out of a sweep's output.
fn rows(text: &str) -> Vec<Row> {
    text.lines()
        .filter_map(|line| line.strip_prefix("MEASURE\t"))
        .filter_map(|line| {
            let mut field = line.split('\t');
            let scenario = field.next()?.to_owned();
            let name = field.next()?.to_owned();
            let unit = field.next()?.to_owned();
            let value = field.next()?.parse().ok()?;
            let limit = field.next()?.parse().ok()?;
            let passed = field.next()? == "ok";
            let rationale = field.next()?.to_owned();
            let budgeted = field.next()?;
            let met = field.next()? == "met";
            let spread = field.next().unwrap_or("-").to_owned();
            Some(Row {
                scenario,
                name,
                unit,
                value,
                limit,
                passed,
                rationale,
                budget: budgeted.parse::<f64>().ok().map(|budget| (budget, met)),
                spread,
            })
        })
        .collect()
}

/// Pulls the pacing lines out of a sweep's output.
fn paces(text: &str) -> Vec<Paced> {
    text.lines()
        .filter_map(|line| line.strip_prefix("PACE\t"))
        .filter_map(|line| {
            let mut field = line.split('\t');
            Some(Paced {
                scenario: field.next()?.to_owned(),
                interval_us: field.next()?.parse().ok()?,
                late: field.next()?.parse().ok()?,
                frames: field.next()?.parse().ok()?,
            })
        })
        .collect()
}

/// Pulls the escalation notes out of a sweep's output.
fn notes(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.strip_prefix("NOTE\t"))
        .filter_map(|line| line.split_once('\t'))
        .map(|(scenario, note)| (scenario.to_owned(), note.to_owned()))
        .collect()
}

/// Pulls the escalation counters out of a sweep's output.
fn escalations(text: &str) -> Vec<Escalated> {
    text.lines()
        .filter_map(|line| line.strip_prefix("ESCALATION\t"))
        .filter_map(|line| {
            let mut field = line.split('\t');
            Some(Escalated {
                scenario: field.next()?.to_owned(),
                emitted: field.next()?.parse().ok()?,
                culled: field.next()?.parse().ok()?,
                inserts: field.next()?.parse().ok()?,
            })
        })
        .collect()
}

/// Seconds since the epoch, which is what one run's file is named after.
fn stamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Runs the sweep, stores it, regenerates the document and reports the verdict.
///
/// # Errors
///
/// Returns the measurements that left their bands, so the caller can exit non-zero naming all of
/// them.
///
/// # Panics
///
/// Panics when a scenario cannot be run at all, or when the stored run cannot be written: a gate
/// that cannot record what it measured has not measured anything a later run can be compared to.
pub(crate) fn write() -> Result<(), Vec<String>> {
    let text = sweep();
    let rows = rows(&text);
    assert!(!rows.is_empty(), "the sweep reported no measurement at all");

    let runs = root().join("docs/perf/runs");
    std::fs::create_dir_all(&runs).expect("the run directory can be created");
    let path = runs.join(format!("{}.tsv", stamp()));
    let stored: String = text
        .lines()
        .filter(|line| {
            line.starts_with("MEASURE\t")
                || line.starts_with("ESCALATION\t")
                || line.starts_with("NOTE\t")
                || line.starts_with("PACE\t")
        })
        .fold(String::new(), |mut into, line| {
            into.push_str(line);
            into.push('\n');
            into
        });
    std::fs::write(&path, stored).expect("the run can be written");
    println!("stored {}", path.display());

    let document = render(
        &rows,
        &escalations(&text),
        &notes(&text),
        &paces(&text),
        history(&runs),
    );
    let performance = root().join("docs/performance.md");
    std::fs::write(&performance, document).expect("the performance document can be written");
    println!("regenerated {}", performance.display());

    let over: Vec<String> = rows
        .iter()
        .filter(|row| !row.passed)
        .map(|row| {
            format!(
                "{} is {:.2} {}, outside its band of {:.2} — {}",
                row.name, row.value, row.unit, row.limit, row.rationale
            )
        })
        .collect();
    if over.is_empty() { Ok(()) } else { Err(over) }
}

/// How many runs are stored, which is the only thing the document says about the ones before this.
fn history(runs: &Path) -> usize {
    std::fs::read_dir(runs).map_or(0, std::iter::Iterator::count)
}

/// Builds `docs/performance.md` from one run.
fn render(
    rows: &[Row],
    escalated: &[Escalated],
    notes: &[(String, String)],
    paced: &[Paced],
    runs: usize,
) -> String {
    let mut out = String::new();
    out.push_str(
        "# Performance\n\
         \n\
         Generated by `cargo xtask perf`, which is a step of `cargo xtask ci`. Every number below \
         was measured by the run that wrote this file; nothing here is typed in by hand, and a \
         number that moved moved this document with it.\n\
         \n\
         Each measurement carries a **band**. A time band is a baseline and a tolerance, because a \
         duration is a property of the machine as much as of the code; a count band is a ceiling \
         with no tolerance at all, because a count is a property of the design and reads the same \
         on a slow machine, a fast one and under a debugger. A run outside any band fails the \
         gate.\n\
         \n",
    );
    let _ = writeln!(
        out,
        "Stored runs: {runs}, under `docs/perf/runs/`, one tab-separated file each.\n"
    );

    for scenario in crate::scenario::ALL {
        let mine: Vec<&Row> = rows.iter().filter(|row| row.scenario == scenario).collect();
        if mine.is_empty() {
            continue;
        }
        let _ = writeln!(out, "## {scenario}\n");
        out.push_str(
            "| measurement | value | unit | distribution | band | verdict | budget | why the band \
             is this wide |\n",
        );
        out.push_str("|---|---:|---|---|---:|---|---|---|\n");
        for row in mine {
            let _ = writeln!(
                out,
                "| `{}` | {:.2} | {} | {} | {:.2} | {} | {} | {} |",
                row.name,
                row.value,
                row.unit,
                if row.spread == "-" {
                    "a count, which has no tail".to_owned()
                } else {
                    row.spread.replace(';', " ")
                },
                row.limit,
                if row.passed { "ok" } else { "**regressed**" },
                match row.budget {
                    None => "—".to_owned(),
                    Some((budget, true)) => format!("{budget:.2} met"),
                    Some((budget, false)) => format!("{budget:.2} **missed**"),
                },
                row.rationale,
            );
        }
        if let Some(pace) = paced.iter().find(|pace| pace.scenario == scenario) {
            let _ = writeln!(
                out,
                "\nFrames delivered: **{} of {} were late** ({:.2} %), against the {:.3} ms \
                 interval this scenario drove them at. A frame is late when it took more than one \
                 and a half of that interval, whatever it cost inside itself.\n",
                pace.late,
                pace.frames,
                if pace.frames == 0 {
                    0.0
                } else {
                    pace.late as f64 * 100.0 / pace.frames as f64
                },
                pace.interval_us / 1000.0,
            );
        }
        out.push('\n');
    }

    let missed: Vec<&Row> = rows
        .iter()
        .filter(|row| matches!(row.budget, Some((_, false))))
        .collect();
    out.push_str("## Budgets\n\n");
    if missed.is_empty() {
        out.push_str(
            "Every measurement that has a budget met it. A budget is what the design was supposed \
             to cost and does not move with the measurement, so this is the statement a band \
             cannot make: nothing here is merely no worse than it was.\n\n",
        );
    } else {
        out.push_str(
            "A budget is what the design was supposed to cost. Unlike a band it does not move with \
             the measurement, so a number can sit inside its band for ever without ever having met \
             its budget — which is why a missed budget is reported here rather than failing the \
             run. Each of these opens an escalation, and the counters in the next section are the \
             evidence it is opened with.\n\n",
        );
        out.push_str("| measurement | measured | budget | over by |\n|---|---:|---:|---:|\n");
        for row in missed {
            let Some((budget, _)) = row.budget else {
                continue;
            };
            let _ = writeln!(
                out,
                "| `{}` | {:.2} {} | {:.2} {} | {:.1}x |",
                row.name,
                row.value,
                row.unit,
                budget,
                row.unit,
                if budget > 0.0 {
                    row.value / budget
                } else {
                    row.value
                },
            );
        }
        out.push('\n');
    }

    out.push_str(
        "## Where the frame's cost is\n\
         \n\
         The three counters below are what decides whether the scene rebuild is the dominant cost \
         of a frame, which is the question a retention scheme would be the answer to. They are \
         recorded for every scenario rather than for the one somebody suspected, because a number \
         that exists only where it was expected proves nothing about anywhere else.\n\
         \n\
         | scenario | primitives emitted | primitives culled | bounds-tree inserts |\n\
         |---|---:|---:|---:|\n",
    );
    for row in escalated {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            row.scenario, row.emitted, row.culled, row.inserts
        );
    }
    if !notes.is_empty() {
        out.push_str(
            "\nAnd what each scenario found that no band expresses — a band says whether something \
             got worse, and these say what the frame is actually doing:\n",
        );
        for (scenario, note) in notes {
            let _ = writeln!(out, "\n- **{scenario}** — {note}");
        }
    }
    out
}
