//! The rule that a suite's pass rate may never fall.
//!
//! # Why the recorded numbers are two, not one
//!
//! A ratchet on the pass *rate* alone is trivially gamed, and not on purpose: deleting a failing
//! test raises the rate, and so does refusing to convert one. So the record holds the number of
//! tests as well, and a run with fewer tests than the record fails however well the survivors did.
//! Together the two say *"at least this many tests, at least this good"*, which is the claim a
//! ratchet is supposed to make.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::wpt::SuiteResult;

/// Where the record lives.
pub fn record_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("suites/ratchet.toml")
}

/// The environment variable that rewrites the record instead of checking it.
pub const BLESS: &str = "ZGUI_BLESS";

/// One suite's recorded floor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Floor {
    /// How many tests the suite held when the record was written.
    pub tests: usize,
    /// How many of them passed.
    pub passing: usize,
}

/// Every suite's recorded floor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Record {
    /// By suite name.
    floors: BTreeMap<String, Floor>,
}

/// A suite that has gone backwards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Regression {
    /// The suite.
    pub suite: String,
    /// What was recorded.
    pub floor: Floor,
    /// What this run measured.
    pub found: Floor,
}

impl core::fmt::Display for Regression {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "`{}` was {}/{} and is now {}/{}",
            self.suite, self.floor.passing, self.floor.tests, self.found.passing, self.found.tests,
        )
    }
}

impl Record {
    /// Reads the record.
    ///
    /// # Errors
    ///
    /// Returns a message when the file is missing or malformed. A missing record is an error rather
    /// than an empty one: an absent floor holds nothing up, and a ratchet that quietly started from
    /// zero would accept any regression at all.
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        Self::parse(&text)
    }

    /// Reads a record from text.
    ///
    /// # Errors
    ///
    /// Returns a message when a suite's entry is not two counts.
    pub fn parse(text: &str) -> Result<Self, String> {
        let document: toml::Table = text.parse().map_err(|error| format!("{error}"))?;
        let mut floors = BTreeMap::new();
        for (suite, value) in document {
            let count = |key: &str| {
                value
                    .get(key)
                    .and_then(toml::Value::as_integer)
                    .and_then(|number| usize::try_from(number).ok())
                    .ok_or_else(|| format!("`{suite}` has no `{key}` count"))
            };
            floors.insert(
                suite.clone(),
                Floor {
                    tests: count("tests")?,
                    passing: count("passing")?,
                },
            );
        }
        if floors.is_empty() {
            return Err("the record names no suites".to_owned());
        }
        Ok(Self { floors })
    }

    /// The record for one suite.
    pub fn floor(&self, suite: &str) -> Option<Floor> {
        self.floors.get(suite).copied()
    }

    /// How many suites are recorded.
    pub fn len(&self) -> usize {
        self.floors.len()
    }

    /// Whether nothing is recorded, which [`Record::parse`] never produces.
    pub fn is_empty(&self) -> bool {
        self.floors.is_empty()
    }

    /// The record this run would write.
    pub fn of(results: &[SuiteResult]) -> Self {
        Self {
            floors: results
                .iter()
                .map(|suite| {
                    (
                        suite.name.clone(),
                        Floor {
                            tests: suite.tests,
                            passing: suite.passing,
                        },
                    )
                })
                .collect(),
        }
    }

    /// The record as the file spells it.
    pub fn to_text(&self) -> String {
        let mut out = String::from(
            "# The floor each converted suite may never fall below.\n\
             #\n\
             # Both numbers are a floor. A run with fewer tests fails even if every one of them\n\
             # passes, because deleting or refusing a failing test is the easiest way to raise a\n\
             # pass rate and the least honest.\n",
        );
        for (suite, floor) in &self.floors {
            out.push_str(&format!(
                "\n[{suite}]\ntests = {}\npassing = {}\n",
                floor.tests, floor.passing
            ));
        }
        out
    }
}

/// Every suite that has gone backwards, or that this run did not measure at all.
pub fn check(record: &Record, results: &[SuiteResult]) -> Vec<Regression> {
    let mut out = Vec::new();
    for (suite, floor) in &record.floors {
        let found = results.iter().find(|result| &result.name == suite).map_or(
            Floor {
                tests: 0,
                passing: 0,
            },
            |result| Floor {
                tests: result.tests,
                passing: result.passing,
            },
        );
        if found.tests < floor.tests || found.passing < floor.passing {
            out.push(Regression {
                suite: suite.clone(),
                floor: *floor,
                found,
            });
        }
    }
    out
}

/// Whether this run has been asked to rewrite the record rather than check it.
pub fn is_blessing() -> bool {
    std::env::var_os(BLESS).is_some_and(|value| !value.is_empty() && value != "0")
}

#[cfg(test)]
mod tests {
    use crate::wpt::SuiteResult;

    use super::{Floor, Record, check, is_blessing, record_path};

    /// A suite result with the given counts and nothing else.
    fn suite(name: &str, tests: usize, passing: usize) -> SuiteResult {
        SuiteResult {
            name: name.to_owned(),
            tests,
            passing,
            unconvertible: 0,
            results: Vec::new(),
        }
    }

    /// The committed record matches what the corpus does today.
    ///
    /// Blessing rewrites it and still fails, so a record is never rewritten by the run that was
    /// supposed to check it.
    #[test]
    fn the_recorded_floor_is_what_the_corpus_reaches() {
        let results = crate::wpt::suite::run_all().expect("the corpus is readable");
        let measured = Record::of(&results);
        if is_blessing() {
            std::fs::write(record_path(), measured.to_text()).expect("the record is writable");
            panic!("rewrote the ratchet record; re-run without the blessing variable to check it");
        }
        let record = Record::load(&record_path()).expect("a committed record");
        assert_eq!(record.len(), results.len());
        assert_eq!(check(&record, &results), Vec::new());
        assert_eq!(record, measured, "the record and the run disagree");
    }

    /// Both halves of the floor hold, and neither can be got round by the other.
    #[test]
    fn the_ratchet_catches_both_ways_of_going_backwards() {
        let record = Record::parse("[flexbox]\ntests = 4\npassing = 3\n").expect("well formed");

        // Fewer passes.
        assert_eq!(check(&record, &[suite("flexbox", 4, 2)]).len(), 1);
        // Fewer tests, and a perfect rate.
        let deleted = check(&record, &[suite("flexbox", 3, 3)]);
        assert_eq!(deleted.len(), 1, "deleting a failing test raises the rate");
        assert_eq!(
            deleted[0].found,
            Floor {
                tests: 3,
                passing: 3
            }
        );
        // The suite disappearing entirely.
        assert_eq!(check(&record, &[]).len(), 1);
        // Improvement is not a regression.
        assert_eq!(check(&record, &[suite("flexbox", 5, 4)]), Vec::new());
    }

    /// A record that names nothing is refused rather than treated as a floor of zero.
    #[test]
    fn an_empty_record_is_an_error() {
        assert!(Record::parse("").is_err());
        assert!(Record::parse("[flexbox]\ntests = 4\n").is_err());
    }
}
