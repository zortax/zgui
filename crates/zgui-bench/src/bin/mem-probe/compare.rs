//! What one more row costs: the difference between two runs, divided by the rows between them.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// One run's figures, read back from the file the probe wrote with `--tsv`.
struct Figures {
    /// How many records the run loaded.
    rows: f64,
    /// The snapshot names, in the order the run took them.
    snapshots: Vec<String>,
    /// The keys, in the order the run wrote them.
    keys: Vec<String>,
    /// Every figure, by snapshot and key.
    values: BTreeMap<(String, String), f64>,
}

/// Reads one run's figures.
fn read(path: &Path) -> Result<Figures, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut figures = Figures {
        rows: 0.0,
        snapshots: Vec::new(),
        keys: Vec::new(),
        values: BTreeMap::new(),
    };
    for line in text.lines() {
        let mut fields = line.split('\t');
        let (Some(snapshot), Some(key), Some(value)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let Ok(value) = value.parse::<f64>() else {
            continue;
        };
        if snapshot == "meta" {
            if key == "rows" {
                figures.rows = value;
            }
            continue;
        }
        if !figures.snapshots.iter().any(|held| held == snapshot) {
            figures.snapshots.push(snapshot.to_owned());
        }
        if !figures.keys.iter().any(|held| held == key) {
            figures.keys.push(key.to_owned());
        }
        figures
            .values
            .insert((snapshot.to_owned(), key.to_owned()), value);
    }
    Ok(figures)
}

/// The keys a per-row table shows: live bytes per tag, then everything that is not a tag.
fn shown(key: &str) -> bool {
    !key.starts_with("tag.") || key.ends_with(".live")
}

/// The per-row table for two runs, `small` and `large`.
///
/// # Errors
///
/// A file that cannot be read, or two runs with the same number of rows.
pub(crate) fn compare(small: &Path, large: &Path) -> Result<String, String> {
    let small = read(small)?;
    let large = read(large)?;
    let rows = large.rows - small.rows;
    if rows == 0.0 {
        return Err("the two runs loaded the same number of rows".to_owned());
    }
    let snapshots: Vec<&String> = large
        .snapshots
        .iter()
        .filter(|name| small.snapshots.contains(name))
        .collect();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "bytes (or objects) per row = (run at {} rows - run at {} rows) / {rows}",
        large.rows, small.rows
    );
    let _ = write!(out, "{:<40}", "");
    for name in &snapshots {
        let _ = write!(out, "{name:>11}");
    }
    out.push('\n');
    for key in large.keys.iter().filter(|key| shown(key)) {
        let per_row: Vec<f64> = snapshots
            .iter()
            .map(|name| {
                let at = |figures: &Figures| {
                    figures
                        .values
                        .get(&((*name).clone(), key.clone()))
                        .copied()
                        .unwrap_or(0.0)
                };
                (at(&large) - at(&small)) / rows
            })
            .collect();
        if per_row.iter().all(|value| value.abs() < 0.05) {
            continue;
        }
        let _ = write!(out, "{key:<40.40}");
        for value in per_row {
            let _ = write!(out, "{value:>11.1}");
        }
        out.push('\n');
    }
    Ok(out)
}
