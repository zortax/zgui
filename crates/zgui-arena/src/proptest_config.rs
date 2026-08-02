//! The settings this crate's property tests share.

use proptest::test_runner::Config;

/// The configuration every property test in this crate runs under.
///
/// It is [`Config::default`], so `PROPTEST_CASES` and the rest of proptest's environment still
/// apply — except under an interpreter, where two of the defaults do not work. Recording a
/// failing case in a file beside the source needs the working directory, which an interpreter
/// running the process in isolation will not hand over; and a case count chosen for compiled code
/// takes hours when every memory access is checked.
pub(crate) fn config() -> Config {
    let mut config = Config::default();
    if cfg!(miri) {
        config.failure_persistence = None;
        config.cases = config.cases.min(8);
    }
    config
}
