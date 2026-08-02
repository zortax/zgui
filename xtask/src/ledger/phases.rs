//! Which phase introduces which crate, read out of `docs/planning/PHASES.md`.
//!
//! The schedule is the source of truth: a crate is assigned to the earliest phase whose
//! *Creates* paragraph names it. Keeping the map derived rather than hand-maintained is what
//! makes the topological check meaningful — a second copy of the schedule would drift.

use std::collections::BTreeMap;
use std::path::Path;

use crate::error::{Result, read_to_string};

/// Crate name to the phase that introduces it.
#[derive(Debug, Clone, Default)]
pub(crate) struct PhaseMap {
    /// The parsed assignments.
    entries: BTreeMap<String, u32>,
}

impl PhaseMap {
    /// Reads `<root>/docs/planning/PHASES.md`, returning an empty map when there is none.
    pub(crate) fn load(root: &Path) -> Result<Self> {
        let path = root.join("docs/planning/PHASES.md");
        if !path.is_file() {
            return Ok(Self::default());
        }
        Ok(Self::parse(&read_to_string(&path)?))
    }

    /// Parses the schedule text.
    pub(crate) fn parse(text: &str) -> Self {
        let mut entries: BTreeMap<String, u32> = BTreeMap::new();
        let mut phase = None;
        let mut in_creates = false;
        for line in text.lines() {
            if let Some(number) = heading_phase(line) {
                phase = Some(number);
                in_creates = false;
                continue;
            }
            if line.trim().is_empty() {
                in_creates = false;
                continue;
            }
            if line.starts_with("**Creates") {
                in_creates = true;
            }
            let Some(phase) = phase.filter(|_| in_creates) else {
                continue;
            };
            for name in crate_names(line) {
                entries
                    .entry(name)
                    .and_modify(|existing| *existing = (*existing).min(phase))
                    .or_insert(phase);
            }
        }
        Self { entries }
    }

    /// The phase that introduces `name`.
    pub(crate) fn phase_of(&self, name: &str) -> Option<u32> {
        self.entries.get(name).copied()
    }

    /// Whether the map holds no assignments at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The highest phase any crate in `names` is assigned to.
    ///
    /// This is how far the tree has been built out, which is what tells the spikes ledger
    /// whether a spike has outlived the phase that was supposed to delete it.
    pub(crate) fn reached_phase<'a>(&self, names: impl Iterator<Item = &'a str>) -> u32 {
        names
            .filter_map(|name| self.phase_of(name))
            .max()
            .unwrap_or(0)
    }
}

/// The phase number of a `## Phase N …` heading.
fn heading_phase(line: &str) -> Option<u32> {
    let rest = line.strip_prefix("## Phase ")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Every crate name written in backticks on a line, ignoring module paths and CSS names.
fn crate_names(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else { break };
        let token = &after[..close];
        if is_crate_name(token) {
            out.push(token.to_owned());
        }
        rest = &after[close + 1..];
    }
    out
}

/// Whether a backticked token names a workspace crate or a spike directory.
fn is_crate_name(token: &str) -> bool {
    let is_plain = |name: &str| {
        !name.is_empty()
            && name.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
    };
    if let Some(spike) = token.strip_prefix("spikes/") {
        return is_plain(spike);
    }
    token.starts_with("zgui") && is_plain(token)
}

#[cfg(test)]
mod tests {
    use super::PhaseMap;

    const SCHEDULE: &str = "\
## Phase 1 — geometry

**Creates.** `zgui-geom` — `unit/{css_px}`.

## Phase 2 — colour

**Creates.** `zgui-color` → `zgui-geom`.

## Phase 6 — Spike A

**Creates.** `spikes/vello-interleave` (`# RETIRE: phase 27`) — a quad pipeline,
`display: block` and other prose.
";

    #[test]
    fn assigns_each_crate_to_its_earliest_phase() {
        let map = PhaseMap::parse(SCHEDULE);
        assert_eq!(map.phase_of("zgui-geom"), Some(1));
        assert_eq!(map.phase_of("zgui-color"), Some(2));
        assert_eq!(map.phase_of("spikes/vello-interleave"), Some(6));
        assert_eq!(map.phase_of("block"), None);
    }

    #[test]
    fn reports_how_far_the_tree_has_been_built() {
        let map = PhaseMap::parse(SCHEDULE);
        assert_eq!(
            map.reached_phase(["zgui-geom", "zgui-color"].into_iter()),
            2
        );
        assert_eq!(map.reached_phase(["probe"].into_iter()), 0);
    }
}
