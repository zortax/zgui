//! Turning the latency trace this run wrote back into a per-stage table.
//!
//! Every consecutive pair of marks is taken mechanically and the gap attributed to the earlier of
//! the two, so a stage nobody thought to name is still visible as the gap in front of the next
//! mark that was. `f.end` is the one exception: the gap after it is the driver's own time between
//! frames and belongs to no stage.

use std::collections::HashMap;

/// Where in the trace file this run's measured part begins.
#[derive(Clone, Copy)]
pub(crate) struct Boundary {
    /// How many bytes the file already held.
    offset: u64,
}

impl Boundary {
    /// The file's length right now, which is where the measured part starts.
    pub(crate) fn here() -> Self {
        let offset = std::env::var("ZGUI_LATENCY")
            .ok()
            .and_then(|path| std::fs::metadata(path).ok())
            .map_or(0, |meta| meta.len());
        Self { offset }
    }
}

/// One mark, as the trace file spells it.
struct Mark {
    /// When, in nanoseconds since the recording's zero.
    at_ns: u128,
    /// What happened.
    stage: String,
    /// What it had to say.
    note: String,
}

/// Pulls a `"key":` field out of one JSON line, without a JSON parser.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let start = line.find(&format!("\"{key}\":"))? + key.len() + 3;
    let rest = &line[start..];
    if let Some(inner) = rest.strip_prefix('"') {
        let end = inner.find('"')?;
        Some(&inner[..end])
    } else {
        let end = rest.find([',', '}'])?;
        Some(&rest[..end])
    }
}

/// What one stage cost across the run.
struct Stage {
    /// Every gap attributed to it, in microseconds.
    samples: Vec<f64>,
}

/// The median of a sorted slice.
fn median(sorted: &[f64]) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[sorted.len() / 2]
}

/// The value below which `fraction` of a sorted slice sits.
fn quantile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() as f64 - 1.0) * fraction).round() as usize;
    sorted[index]
}

/// Reads back the trace and prints the per-stage table.
pub(crate) fn report(size: &str, phase: &str, boxes: usize, fragments: usize, mark: Boundary) {
    let Ok(path) = std::env::var("ZGUI_LATENCY") else {
        eprintln!("no ZGUI_LATENCY: no per-stage table");
        return;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let measured = text.get(mark.offset as usize..).unwrap_or("");
    let marks: Vec<Mark> = measured
        .lines()
        .filter_map(|line| {
            Some(Mark {
                at_ns: field(line, "t")?.parse().ok()?,
                stage: field(line, "stage")?.to_owned(),
                note: field(line, "note").unwrap_or("").to_owned(),
            })
        })
        .collect();

    let mut stages: HashMap<String, Stage> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut frames: Vec<f64> = Vec::new();
    let mut frame_start: Option<u128> = None;
    for pair in marks.windows(2) {
        let (here, next) = (&pair[0], &pair[1]);
        if here.stage == "f.begin" {
            frame_start = Some(here.at_ns);
        }
        if here.stage == "f.end" {
            if let Some(start) = frame_start.take() {
                frames.push((here.at_ns - start) as f64 / 1000.0);
            }
            continue;
        }
        let gap = (next.at_ns.saturating_sub(here.at_ns)) as f64 / 1000.0;
        let entry = stages.entry(here.stage.clone()).or_insert_with(|| {
            order.push(here.stage.clone());
            Stage {
                samples: Vec::new(),
            }
        });
        entry.samples.push(gap);
    }

    frames.sort_by(f64::total_cmp);
    let mut rows: Vec<(String, f64, f64, f64, f64, usize)> = order
        .iter()
        .map(|name| {
            let stage = &stages[name];
            let mut samples = stage.samples.clone();
            samples.sort_by(f64::total_cmp);
            let total: f64 = samples.iter().sum();
            (
                name.clone(),
                total,
                median(&samples),
                quantile(&samples, 0.95),
                total / samples.len() as f64,
                samples.len(),
            )
        })
        .collect();
    rows.sort_by(|left, right| right.1.total_cmp(&left.1));

    println!("STAGES size={size} phase={phase} boxes={boxes} frags={fragments}");
    println!(
        "  frames={} frame_median_us={:.1} frame_mean_us={:.1} frame_p95_us={:.1} \
         frame_max_us={:.1}",
        frames.len(),
        median(&frames),
        frames.iter().sum::<f64>() / frames.len().max(1) as f64,
        quantile(&frames, 0.95),
        frames.last().copied().unwrap_or(0.0)
    );
    println!(
        "  {:<22} {:>11} {:>11} {:>11} {:>11} {:>7}",
        "stage", "total_us", "median_us", "p95_us", "mean_us", "n"
    );
    for (name, total, med, p95, mean, count) in &rows {
        if *total < 1.0 {
            continue;
        }
        println!("  {name:<22} {total:>11.1} {med:>11.2} {p95:>11.2} {mean:>11.2} {count:>7}");
    }

    // The roll-up. `lay_out` and `build_boxes` are called from two places — once from the frame
    // and again from every observation-delivery pass — and only the frame's own call is bracketed
    // by marks, so the naive per-mark table charges a re-layout's taffy pass to whichever mark
    // happened to precede it. This walks the frames and separates the two.
    let mut per_frame: Vec<(f64, f64, f64, f64, usize)> = Vec::new();
    let mut open: Option<(f64, f64, f64, f64, usize)> = None;
    for pair in marks.windows(2) {
        let (here, next) = (&pair[0], &pair[1]);
        let gap = (next.at_ns.saturating_sub(here.at_ns)) as f64 / 1000.0;
        if here.stage == "f.begin" {
            if let Some(frame) = open.take() {
                per_frame.push(frame);
            }
            open = Some((0.0, 0.0, 0.0, 0.0, 0));
        }
        let Some(frame) = open.as_mut() else { continue };
        match here.stage.as_str() {
            // The frame's own build, bracketed on both sides.
            "b.why" if next.stage == "f.layout" => frame.0 += gap,
            // A re-layout's build and taffy pass together, with no mark between them.
            "b.why" => {
                frame.0 += gap;
                frame.4 += 1;
            }
            "f.layout" => frame.1 += gap,
            "f.fragments" => frame.2 += gap,
            "p.emit" => frame.3 += gap,
            "w.laidout" => frame.4 += 0,
            _ => {}
        }
        if here.stage == "f.end"
            && let Some(frame) = open.take()
        {
            per_frame.push(frame);
        }
    }
    if let Some(frame) = open.take() {
        per_frame.push(frame);
    }
    let mut laidout_per_frame: Vec<f64> = Vec::new();
    {
        let mut count = 0.0;
        for mark in &marks {
            if mark.stage == "f.begin" {
                laidout_per_frame.push(count);
                count = 0.0;
            }
            if mark.stage == "w.laidout" {
                count += 1.0;
            }
        }
        laidout_per_frame.push(count);
        laidout_per_frame.retain(|value| *value > 0.0);
        laidout_per_frame.sort_by(f64::total_cmp);
    }
    for (index, label) in ["boxtree_build", "taffy", "fragment_diff", "emit"]
        .into_iter()
        .enumerate()
    {
        let mut values: Vec<f64> = per_frame
            .iter()
            .map(|frame| match index {
                0 => frame.0,
                1 => frame.1,
                2 => frame.2,
                _ => frame.3,
            })
            .collect();
        values.sort_by(f64::total_cmp);
        println!(
            "  ROLLUP {label:<14} median_us={:>10.2} p95_us={:>10.2} mean_us={:>10.2}",
            median(&values),
            quantile(&values, 0.95),
            values.iter().sum::<f64>() / values.len().max(1) as f64
        );
    }
    println!(
        "  ROLLUP {:<14} median={:.1} max={:.1}",
        "layouts/frame",
        median(&laidout_per_frame),
        laidout_per_frame.last().copied().unwrap_or(0.0)
    );

    // The notes that say how much work a frame decided to do, rather than how long it took.
    for wanted in [
        "f.restyled",
        "d.prelayout",
        "d.postexpand",
        "p.glyphs",
        "f.end",
    ] {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for mark in marks.iter().filter(|mark| mark.stage == wanted) {
            *counts.entry(mark.note.as_str()).or_default() += 1;
        }
        let mut seen: Vec<(&&str, &usize)> = counts.iter().collect();
        seen.sort_by(|left, right| right.1.cmp(left.1));
        for (note, count) in seen.into_iter().take(3) {
            let short: String = note.chars().take(150).collect();
            println!("  NOTE {wanted:<14} x{count:<5} {short}");
        }
    }
}

/// What one *interaction* cost, as opposed to what one frame cost.
///
/// A user-visible interaction is an input event plus every frame the loop ran before it was quiet
/// again. That is the quantity the user feels, and it is not the per-frame median: a wheel notch
/// that costs 33 ms in one frame and a keystroke that costs 90 µs across three frames are both one
/// interaction. Each row is one, with the frames it took and every counter that moved inside it.
pub(crate) struct Ledger {
    /// What the interaction is called.
    label: String,
    /// One row per interaction.
    rows: Vec<Row>,
}

/// One interaction.
struct Row {
    /// How long the whole thing took, in microseconds.
    us: f64,
    /// How many frames the loop ran for it.
    frames: u64,
    /// What every counter did while it ran.
    counters: zgui_profile::Counters,
}

impl Ledger {
    /// An empty ledger for an interaction called `label`.
    pub(crate) fn new(label: &str) -> Self {
        Self {
            label: label.to_owned(),
            rows: Vec::new(),
        }
    }

    /// Records one interaction.
    pub(crate) fn push(
        &mut self,
        elapsed: std::time::Duration,
        frames: u64,
        counters: zgui_profile::Counters,
    ) {
        self.rows.push(Row {
            us: elapsed.as_secs_f64() * 1e6,
            frames,
            counters,
        });
    }

    /// Prints the table.
    pub(crate) fn report(&self, size: &str, boxes: usize) {
        if self.rows.is_empty() {
            return;
        }
        let mut times: Vec<f64> = self.rows.iter().map(|row| row.us).collect();
        times.sort_by(f64::total_cmp);
        let frames: u64 = self.rows.iter().map(|row| row.frames).sum();
        let count = self.rows.len() as f64;
        println!(
            "LEDGER size={size} boxes={boxes} interaction={} n={} \
             us_median={:.2} us_p95={:.2} us_mean={:.2} us_max={:.2} frames_per={:.2}",
            self.label,
            self.rows.len(),
            median(&times),
            quantile(&times, 0.95),
            times.iter().sum::<f64>() / count,
            times.last().copied().unwrap_or(0.0),
            frames as f64 / count,
        );
        let total = self
            .rows
            .iter()
            .fold(zgui_profile::Counters::ZERO, |acc, row| {
                add(acc, &row.counters)
            });
        for (counter, value) in total.iter() {
            if value == 0 {
                continue;
            }
            println!(
                "  CTR {:<26} per_interaction={:>12.2} per_frame={:>12.2} per_box={:>8.4}",
                counter.name(),
                value as f64 / count,
                value as f64 / frames.max(1) as f64,
                value as f64 / count / boxes.max(1) as f64,
            );
        }
    }
}

/// Adds every counter of `right` into `left`.
fn add(left: zgui_profile::Counters, right: &zgui_profile::Counters) -> zgui_profile::Counters {
    zgui_profile::Counters::from_fn(|counter| left.get(counter) + right.get(counter))
}
