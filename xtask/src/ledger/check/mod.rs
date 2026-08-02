//! The ledger checks, and the registry that names them.

pub(crate) mod attribution;
pub(crate) mod clock;
pub(crate) mod counters;
pub(crate) mod engines;
pub(crate) mod ignored;
pub(crate) mod inert;
pub(crate) mod mutation;
pub(crate) mod pinned;
pub(crate) mod skips;
pub(crate) mod spikes;
pub(crate) mod tag_syntax;
pub(crate) mod topo;
pub(crate) mod unsafe_code;
pub(crate) mod versions;

use crate::ledger::report::Report;
use crate::ledger::tree::Tree;

/// One check: its name on the command line, what it asserts, and how to run it.
pub(crate) struct Check {
    /// The name `cargo xtask ledger <name>` selects.
    pub(crate) name: &'static str,
    /// One line describing what the check asserts.
    pub(crate) description: &'static str,
    /// The check itself.
    pub(crate) run: fn(&Tree) -> Report,
}

/// Every check, in the order they run.
pub(crate) const CHECKS: &[Check] = &[
    Check {
        name: "engines",
        description: "each external engine is named only by the crates permitted to name it",
        run: engines::check,
    },
    Check {
        name: "unsafe",
        description: "unsafe is forbidden outside the allowlist, and every Sync promise states its reason",
        run: unsafe_code::check,
    },
    Check {
        name: "attribution",
        description: "adapted code carries its licence header and a matching NOTICE row",
        run: attribution::check,
    },
    Check {
        name: "versions",
        description: "the pinned versions, the single wgpu, and the feature sets that fail silently",
        run: versions::check,
    },
    Check {
        name: "pinned",
        description: "the two dependency lists that are pinned exactly, and the edge that must not exist",
        run: pinned::check,
    },
    Check {
        name: "topo",
        description: "the phase schedule is a topological order of the real manifest graph",
        run: topo::check,
    },
    // Before the two checks that read the suite, because both of them are satisfied by a test that
    // exists and neither by one that runs.
    Check {
        name: "ignored",
        description: "no test is switched off, so a gate satisfied by one is satisfied by a test that runs",
        run: ignored::check,
    },
    Check {
        name: "counters",
        description: "every frame counter is incremented by something, or listed as awaiting its stage",
        run: counters::check,
    },
    Check {
        name: "skips",
        description: "every counter of avoided work names its pair and has a non-vacuity test",
        run: skips::check,
    },
    Check {
        name: "clock",
        description: "every wall-clock assertion lives in a target the gate runs in release",
        run: clock::check,
    },
    Check {
        name: "mutation",
        description: "a document is changed only through its own batch API, never through the arena",
        run: mutation::check,
    },
    Check {
        name: "inert",
        description: "every enum variant something branches on is built by something",
        run: inert::check,
    },
    Check {
        name: "tag-syntax",
        description: "no view, in code or in a document, is written in the spelling the grammar replaced",
        run: tag_syntax::check,
    },
    Check {
        name: "spikes",
        description: "every spike names the phase that deletes it, and none outlives it",
        run: spikes::check,
    },
];

/// Looks a check up by name.
pub(crate) fn find(name: &str) -> Option<&'static Check> {
    CHECKS.iter().find(|check| check.name == name)
}
