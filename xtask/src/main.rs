//! The workspace's gate runner.
//!
//! `cargo xtask ci` runs the full definition of done; `cargo xtask lint` runs clippy over the
//! workspace in both profiles, which `ci` also does; `cargo xtask wall-clock` runs the wall-clock
//! budgets in an optimised build, which `ci` also does; `cargo xtask perf` runs the five end-to-end
//! scenarios against their bands and regenerates `docs/performance.md`, which `ci` also does;
//! `cargo xtask docs` runs the documentation gate; `cargo xtask release` checks that the tree can
//! be published in lockstep and that its public surface still keeps the last release's promise;
//! `cargo xtask skips` checks that every counter of avoided work carries the assertion that proves
//! it can move; `cargo xtask budget` runs the cache-eviction gate; `cargo xtask resize` measures
//! the resize slope against a same-run baseline; `cargo xtask cadence` settles the frame rate of an
//! animation and of the overscroll spring on three outputs; `cargo xtask workloads` runs the
//! reference workloads against the same-run ratios recorded for them; `cargo xtask hits` and
//! `cargo xtask a11y-geom` hold a running window against one that computed the frame from nothing,
//! on what is under a point and on the rectangles handed to a screen reader and an input method;
//! `cargo xtask verify` holds the same two windows against each other on the display list they
//! finished with and on every fragment's resolved border box, at every document size; each of those
//! is also a step of `ci`.
//! `cargo xtask growth` drives a thousand scroll ticks and fails on any live count that is larger
//! at the end than it was after ten of them; `cargo xtask tails` reads the run the ratchet last
//! stored and fails on any duration published without its distribution; both are also steps of
//! `ci`. `cargo xtask perf pacing` measures a scripted scroll's frame intervals and is deliberately
//! not one, because it measures on the real output or it does not measure.
//! `cargo xtask ledger` runs the mechanical ledgers on their own; `cargo xtask tsan` runs the
//! thread sanitiser over the threaded targets together with the control that proves it was
//! watching; `cargo xtask new-crate` stamps out a member that already satisfies the ledgers.

#![forbid(unsafe_code)]

mod budget;
mod cadence;
mod ci;
mod cli;
mod docs;
mod error;
mod gate;
mod growth;
mod ledger;
mod lint;
mod new_crate;
mod oracle;
mod perf;
mod process;
mod release;
mod resize;
mod root;
mod skips;
mod tails;
mod tsan;
mod verify;
mod wall_clock;
mod workloads;

use std::process::ExitCode;

use crate::cli::Command;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match Command::parse(&args) {
        Ok(command) => command,
        Err(message) => {
            eprintln!("xtask: {message}\n\n{}", cli::USAGE);
            return ExitCode::FAILURE;
        }
    };

    let root = match root::workspace_root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("xtask: {error}");
            return ExitCode::FAILURE;
        }
    };

    let outcome = match command {
        Command::Ci => ci::run(&root),
        Command::Lint => lint::run(&root),
        Command::WallClock => wall_clock::run(&root),
        Command::Perf => perf::run(&root),
        Command::PerfPacing { size, seconds } => perf::pacing(&root, &size, seconds),
        Command::Growth => growth::run(&root),
        Command::Tails => tails::run(&root),
        Command::Skips => skips::run(&root),
        Command::Budget => budget::run(&root),
        Command::Resize => resize::run(&root),
        Command::Cadence => cadence::run(&root),
        Command::Workloads => workloads::run(&root),
        Command::Hits => oracle::run(&root, "hits"),
        Command::A11yGeom => oracle::run(&root, "a11y-geom"),
        Command::Verify => verify::run(&root),
        Command::Docs => docs::run(&root),
        Command::Release => release::run(&root),
        Command::Ledger { only } => ledger::run(&root, only.as_deref()),
        Command::LedgerSelfTest => ledger::self_test::run(&root),
        Command::Tsan => tsan::run(&root),
        Command::NewCrate { name, layer } => new_crate::run(&root, &name, layer),
        Command::Help => {
            println!("{}", cli::USAGE);
            Ok(())
        }
    };

    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("\nxtask: {error}");
            ExitCode::FAILURE
        }
    }
}
