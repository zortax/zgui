//! The feature flags, and the one thing that proves they matter.
//!
//! This target runs in a process of its own, which is what lets it observe the flags *before* they
//! are set. Asserting only that they read back `true` afterwards would pass on a build where every
//! flag defaulted to `true` and the whole start-up did nothing.
//!
//! It is deliberately a single test. The flags are process-global, so a second test in this target
//! would race the first for the one transition there is to watch.

use zgui_css::parity::{EngineStatus, status_of};
use zgui_css::prefs::{enable_css_features, feature_flags};

#[test]
fn start_up_turns_on_every_flag_and_the_parser_hears_about_it() {
    let before = feature_flags();
    // The flips and the read-back come from one list, so this is the count of flags actually set,
    // not a restatement of the return type: a twenty-fifth flip fails here until it is accounted
    // for.
    assert_eq!(before.len(), 24, "the flag list is the one being set");
    let already_on: Vec<&str> = before
        .into_iter()
        .filter_map(|(name, on)| on.then_some(name))
        .collect();
    assert!(
        already_on.is_empty(),
        "these start on, so turning them on proves nothing: {already_on:?}"
    );

    // `text-overflow` is generated but gated, which makes it the property that shows a flag
    // reaching the *parser* rather than merely reaching an atomic. A sheet parsed while the flag is
    // off loses the declaration without reporting anything, which is the failure this whole
    // start-up exists to prevent.
    assert_eq!(
        status_of("text-overflow"),
        EngineStatus::Longhand { enabled: false },
        "the property is generated in this build, and only its flag decides whether a sheet may use it"
    );

    enable_css_features();

    let missed: Vec<&str> = feature_flags()
        .into_iter()
        .filter_map(|(name, on)| (!on).then_some(name))
        .collect();
    assert!(
        missed.is_empty(),
        "start-up left these off, so they are read but never set: {missed:?}"
    );
    assert_eq!(
        status_of("text-overflow"),
        EngineStatus::Longhand { enabled: true }
    );
}
