//! Running a named set of assertions, and refusing to run one that has gone.
//!
//! Several gates are made of assertions that already live beside the code they are about, which is
//! where a reader who broke one finds it. What a gate adds is that the assertions are *named*: a
//! gate defined as "run this target" is green when the target has been emptied, green when its one
//! assertion was renamed away by a change that meant to keep it, and green when somebody deleted
//! the file. So the names are listed, checked against what the target says it holds, and only then
//! is the target run.

mod subject;

use std::path::Path;

use crate::error::{Error, Result};
use crate::process;

pub(crate) use crate::gate::subject::Subject;

/// Runs every subject of the gate called `gate`, after checking each still holds its assertions.
pub(crate) fn run(root: &Path, gate: &str, subjects: &[Subject]) -> Result<()> {
    let cargo = process::cargo();
    for subject in subjects {
        println!("{gate} {:<16} {}", subject.target, subject.about);
        let listed = list(root, &cargo, subject)?;
        for required in subject.required {
            if !listed.iter().any(|name| name == required) {
                return Err(Error::failed(subject.missing(gate, required, &listed)));
            }
        }
        process::run(
            root,
            &cargo,
            &["test", "-p", subject.member, "--test", subject.target],
            &[],
        )?;
    }
    Ok(())
}

/// The test names one target holds, taken from the target itself.
fn list(root: &Path, cargo: &str, subject: &Subject) -> Result<Vec<String>> {
    let output = process::capture(
        root,
        cargo,
        &[
            "test",
            "-p",
            subject.member,
            "--test",
            subject.target,
            "--",
            "--list",
            "--format",
            "terse",
        ],
    )?;
    Ok(output
        .lines()
        .filter_map(|line| line.strip_suffix(": test"))
        .map(str::to_owned)
        .collect())
}
