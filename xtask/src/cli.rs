//! Hand-rolled argument parsing, so the gate runner needs no argument-parsing dependency.

use crate::new_crate::Layer;

/// The text printed for `cargo xtask help` and for any malformed invocation.
pub(crate) const USAGE: &str = "\
usage:
  cargo xtask ci                      run the full definition of done
  cargo xtask lint                    run clippy over every target in both profiles,
                                      which is also a step of `ci`
  cargo xtask wall-clock              run the wall-clock budget targets in release,
                                      which is also a step of `ci`
  cargo xtask perf                    run the five end-to-end scenarios against their
                                      bands, store the run and regenerate
                                      docs/performance.md, which is also a step of `ci`
  cargo xtask perf pacing [size] [s]  measure a scripted scroll's frame intervals as a
                                      distribution, with its late-interval fraction, its
                                      missed vsyncs and its last-second/first-second ratio.
                                      Run by hand at phase exit on the reference machine;
                                      deliberately NOT a step of `ci`, because it measures
                                      on the real output or it does not measure
  cargo xtask growth                  check that every live count after a thousand scroll
                                      ticks equals its value after ten, which is also a
                                      step of `ci`
  cargo xtask tails                   check that every measurement the ratchet published
                                      carries p50, p95, p99 and max, and that every
                                      scenario published its late-frame count against the
                                      interval it ran at, which is also a step of `ci`
  cargo xtask skips                   check that every counter of avoided work names its pair
                                      and carries the assertion that proves it can move,
                                      which is also a step of `ci`
  cargo xtask budget                  check that a cache over its soft limit comes back under it
                                      and that no replayed range names an evicted tile,
                                      which is also a step of `ci`
  cargo xtask resize                  measure the resize slope against a baseline taken in the
                                      same run, which is also a step of `ci`
  cargo xtask cadence                 check that an animation and the overscroll spring each get
                                      one frame per refresh at 60, 75 and 240 Hz,
                                      which is also a step of `ci`
  cargo xtask workloads               run the reference workloads against the same-run ratios
                                      recorded for them, which is also a step of `ci`
  cargo xtask hits                    check that what is under a point is the same in a running
                                      window and in one that computed the frame from nothing,
                                      which is also a step of `ci`
  cargo xtask a11y-geom               check the same for the rectangles handed to a screen reader
                                      and to an input method, which is also a step of `ci`
  cargo xtask verify                  check that the display list and the resolved geometry of a
                                      running window are those of one rebuilt from nothing, at
                                      every document size, which is also a step of `ci`
  cargo xtask docs                    run the documentation gate, which is also a step of `ci`
  cargo xtask release                 check that the tree can be released in lockstep and that
                                      its public surface still keeps the last release's promise,
                                      which is also a step of `ci`
  cargo xtask ledger [check]          run every ledger check, or just one of
                                      engines, unsafe, attribution, versions, pinned,
                                      topo, counters, skips, clock, mutation, inert,
                                      tag-syntax, spikes
  cargo xtask ledger --self-test      prove every check fails on its planted fixture
  cargo xtask tsan                    run the thread sanitiser over the threaded targets,
                                      and its positive control, which must fire
  cargo xtask new-crate <name> --layer <L0..L8>
                                      stamp out a workspace member that already passes the ledgers
  cargo xtask help                    print this text";

/// A parsed command line.
#[derive(Debug, PartialEq)]
pub(crate) enum Command {
    /// Run every gate in the definition of done.
    Ci,
    /// Run clippy over every target of every member, in both profiles.
    Lint,
    /// Run the wall-clock budget targets in an optimised build.
    WallClock,
    /// Run the end-to-end performance scenarios against their bands.
    Perf,
    /// Measure a scripted scroll's pacing, by hand, on the real output.
    PerfPacing {
        /// Which document size to drive.
        size: String,
        /// How many seconds to drive it for.
        seconds: f64,
    },
    /// Check that no live count grew across a thousand scroll ticks.
    Growth,
    /// Check that every published duration carries its distribution.
    Tails,
    /// Check that every counter of avoided work carries its pair and its non-vacuity assertion.
    Skips,
    /// Check the caches against their soft limits and against what replays name.
    Budget,
    /// Measure the resize slope against a baseline taken in the same run.
    Resize,
    /// Check the frame rate of an animation and of the overscroll spring on three outputs.
    Cadence,
    /// Run the reference workloads against the same-run ratios recorded for them.
    Workloads,
    /// Check what is under a point against a window that computed the frame from nothing.
    Hits,
    /// Check the published rectangles against the same.
    A11yGeom,
    /// Check the display list and the resolved geometry against the same.
    Verify,
    /// Run the documentation gate.
    Docs,
    /// Run the lockstep and public-surface compatibility gates.
    Release,
    /// Run the ledger checks, optionally narrowed to one of them.
    Ledger {
        /// The single check to run, or `None` for all of them.
        only: Option<String>,
    },
    /// Run the ledger checks against their planted-violation fixtures.
    LedgerSelfTest,
    /// Run the thread sanitiser over the threaded targets and its positive control.
    Tsan,
    /// Create a new workspace member from the template.
    NewCrate {
        /// The crate name, which is also its directory name under `crates/`.
        name: String,
        /// The layer the crate belongs to.
        layer: Layer,
    },
    /// Print the usage text.
    Help,
}

impl Command {
    /// Parses the arguments that follow `cargo xtask`.
    ///
    /// The error is a human-readable reason, printed above the usage text.
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        let mut args = args.iter().map(String::as_str);
        match args.next() {
            None | Some("help" | "-h" | "--help") => Ok(Self::Help),
            Some("ci") => Ok(Self::Ci),
            Some("lint") => Ok(Self::Lint),
            Some("wall-clock") => Ok(Self::WallClock),
            Some("perf") => match args.next() {
                None => Ok(Self::Perf),
                Some("pacing") => Ok(Self::PerfPacing {
                    size: args.next().unwrap_or("s13").to_owned(),
                    seconds: args
                        .next()
                        .map_or(Ok(60.0), str::parse)
                        .map_err(|_| "pacing's seconds must be a number".to_owned())?,
                }),
                Some(other) => Err(format!("unknown perf option `{other}`")),
            },
            Some("growth") => Ok(Self::Growth),
            Some("tails") => Ok(Self::Tails),
            Some("skips") => Ok(Self::Skips),
            Some("budget") => Ok(Self::Budget),
            Some("resize") => Ok(Self::Resize),
            Some("cadence") => Ok(Self::Cadence),
            Some("workloads") => Ok(Self::Workloads),
            Some("hits") => Ok(Self::Hits),
            Some("a11y-geom") => Ok(Self::A11yGeom),
            Some("verify") => Ok(Self::Verify),
            Some("docs") => Ok(Self::Docs),
            Some("release") => Ok(Self::Release),
            Some("ledger") => match args.next() {
                None => Ok(Self::Ledger { only: None }),
                Some("--self-test") => Ok(Self::LedgerSelfTest),
                Some(name) if !name.starts_with('-') => Ok(Self::Ledger {
                    only: Some(name.to_owned()),
                }),
                Some(other) => Err(format!("unknown ledger option `{other}`")),
            },
            Some("tsan") => Ok(Self::Tsan),
            Some("new-crate") => parse_new_crate(args),
            Some(other) => Err(format!("unknown command `{other}`")),
        }
    }
}

/// Parses `new-crate <name> --layer <layer>`.
fn parse_new_crate<'a>(mut args: impl Iterator<Item = &'a str>) -> Result<Command, String> {
    let name = args
        .next()
        .ok_or_else(|| "new-crate needs a crate name".to_owned())?;
    let mut layer = None;
    while let Some(argument) = args.next() {
        match argument {
            "--layer" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--layer needs a value, for example L4".to_owned())?;
                layer = Some(value.parse::<Layer>()?);
            }
            other => return Err(format!("unknown new-crate option `{other}`")),
        }
    }
    let layer = layer.ok_or_else(|| "new-crate needs --layer <L0..L8>".to_owned())?;
    Ok(Command::NewCrate {
        name: name.to_owned(),
        layer,
    })
}

#[cfg(test)]
mod tests {
    use super::{Command, Layer};

    fn parse(line: &str) -> Result<Command, String> {
        let args: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        Command::parse(&args)
    }

    #[test]
    fn parses_every_command_form() {
        assert_eq!(parse("ci"), Ok(Command::Ci));
        assert_eq!(parse("lint"), Ok(Command::Lint));
        assert_eq!(parse("wall-clock"), Ok(Command::WallClock));
        assert_eq!(parse("perf"), Ok(Command::Perf));
        assert_eq!(
            parse("perf pacing"),
            Ok(Command::PerfPacing {
                size: "s13".to_owned(),
                seconds: 60.0
            })
        );
        assert_eq!(
            parse("perf pacing s3 12"),
            Ok(Command::PerfPacing {
                size: "s3".to_owned(),
                seconds: 12.0
            })
        );
        assert_eq!(parse("growth"), Ok(Command::Growth));
        assert_eq!(parse("tails"), Ok(Command::Tails));
        assert_eq!(parse("skips"), Ok(Command::Skips));
        assert_eq!(parse("budget"), Ok(Command::Budget));
        assert_eq!(parse("resize"), Ok(Command::Resize));
        assert_eq!(parse("cadence"), Ok(Command::Cadence));
        assert_eq!(parse("workloads"), Ok(Command::Workloads));
        assert_eq!(parse("hits"), Ok(Command::Hits));
        assert_eq!(parse("a11y-geom"), Ok(Command::A11yGeom));
        assert_eq!(parse("docs"), Ok(Command::Docs));
        assert_eq!(parse("release"), Ok(Command::Release));
        assert_eq!(parse("ledger"), Ok(Command::Ledger { only: None }));
        assert_eq!(
            parse("ledger engines"),
            Ok(Command::Ledger {
                only: Some("engines".to_owned())
            })
        );
        assert_eq!(parse("ledger --self-test"), Ok(Command::LedgerSelfTest));
        assert_eq!(parse("tsan"), Ok(Command::Tsan));
        assert_eq!(
            parse("new-crate zgui-paint --layer L4"),
            Ok(Command::NewCrate {
                name: "zgui-paint".to_owned(),
                layer: Layer::L4
            })
        );
        assert_eq!(parse(""), Ok(Command::Help));
    }

    #[test]
    fn rejects_nonsense() {
        assert!(parse("frobnicate").is_err());
        assert!(parse("new-crate zgui-paint").is_err());
        assert!(parse("new-crate zgui-paint --layer L9").is_err());
        assert!(parse("ledger --wat").is_err());
        assert!(parse("perf --wat").is_err());
        assert!(parse("perf pacing s3 later").is_err());
    }
}
