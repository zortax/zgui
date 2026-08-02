//! What the two oracles run, at which document sizes, and what each size must still state.
//!
//! And, beside each of them, the tests that cover what a differential structurally cannot — see
//! [`guard`](super::guard) for why a comparison of two windows is silent about an error both of
//! them make.

use super::guard::Guard;

/// Where this list lives, for a failure that says what to edit.
pub(crate) const HERE: &str = "xtask/src/oracle/subject.rs";

/// One document size a differential is run at, and the criteria it owes there.
pub(crate) struct Size {
    /// The gallery size, as the harness names it.
    pub(crate) name: &'static str,
    /// The criteria the run must state a verdict for, by the name it prints them under.
    pub(crate) required: &'static [&'static str],
    /// How many steps of the script this document is recorded as declining to compare.
    ///
    /// Read back exactly, in both directions — see [`skipped`](super::skipped) for why a floor of
    /// "more compared than skipped" is not a bound at all.
    ///
    /// It is zero at every document, which is the strongest this field can say: the running window
    /// and the one rebuilt from nothing reach the same layout at all ninety-five steps, so every
    /// step is a comparison rather than an abstention. A document that starts declining one again
    /// fails the gate rather than quietly narrowing it.
    pub(crate) skipped: usize,
}

/// One oracle: a differential phase of the harness, run over several documents.
pub(crate) struct Oracle {
    /// The gate's name, which is also the subcommand's.
    pub(crate) gate: &'static str,
    /// The phase inside the harness.
    pub(crate) phase: &'static str,
    /// What it holds a live window against a cold one about, in one sentence a failure can quote.
    pub(crate) about: &'static str,
    /// The documents it runs over.
    pub(crate) sizes: &'static [Size],
    /// The tests that cover the correctness this gate cannot see, checked to still exist.
    ///
    /// Not a reading list. A green run of this gate means the live window agrees with a rebuild;
    /// whether either of them is *right* is decided by these, and a gate that named none of them
    /// would be claiming a coverage nothing in the workspace provides.
    pub(crate) guarded_by: &'static [Guard],
}

/// The criterion both oracles state at every size.
const HITS: &[&str] = &["hit_results_agree_with_a_cold_window"];

/// The same for the published rectangles, where there is nothing editable on the page.
const GEOMETRY: &[&str] = &["a11y_and_caret_geometry_agree_with_a_cold_window"];

/// And where there is: the caret half states itself, so a document that stopped planning one
/// cannot leave this gate green over a comparison of nothing against nothing.
const GEOMETRY_AND_CARET: &[&str] = &[
    "a11y_and_caret_geometry_agree_with_a_cold_window",
    "caret_geometry_agrees_with_a_cold_window",
];

/// The four documents each oracle is run over.
///
/// The shell alone, one section, two, and the shipped gallery. The small ones are not there for
/// speed: a page with a handful of things on it is where a differential is likeliest to be
/// comparing nothing, and a sweep that only ever runs the full document never visits that. The
/// full one is where the coordinate systems are — the transforms, the sticky headers and the
/// overlays all live in sections the smaller documents do not hold.
///
/// `s0` and `s1` have no editable element anywhere on the page, so no step of the script puts a
/// caret on either of them and neither owes the caret criterion. `s2` is the first document with a
/// text field in it.
const DOCUMENTS: [&str; 4] = ["s0", "s1", "s2", "s13"];

/// What decides whether a hit answer is *right*, as against merely stable.
///
/// Both were watched failing: the first when the clip chain is tested in the fragment's own space,
/// the second when the point is not carried into the coordinate system at all. Neither mutation
/// moves the `hits` gate off green, because both windows make the same mistake in the same place.
const HITS_GUARDS: &[Guard] = &[
    Guard {
        file: "crates/zgui-layout/tests/fragments/hits.rs",
        test: "a_transformed_box_answers_only_where_its_ancestors_clip_shows_it",
        mutation: "use the fragment's own space for the clip test in `HitIndex::hit`",
    },
    Guard {
        file: "crates/zgui-layout/tests/fragments/hits.rs",
        test: "a_transformed_fragment_is_hit_where_it_appears",
        mutation: "answer the query in the fragment's untransformed rectangle",
    },
];

/// What decides whether a published rectangle is right.
///
/// The first was watched failing when `bounds_of` resolves against `Placements::EMPTY` — which is
/// the exact regression the `a11y-geom` gate's own preamble once claimed to catch, and does not.
/// The second is the same question for the rectangle an input method is handed.
const GEOMETRY_GUARDS: &[Guard] = &[
    Guard {
        file: "crates/zgui-runtime/tests/a11y.rs",
        test: "a_control_under_a_transform_is_reported_where_the_transform_puts_it",
        mutation: "resolve `project::geometry::bounds_of` against `Placements::EMPTY`",
    },
    Guard {
        file: "crates/zgui-runtime/tests/editing.rs",
        test: "an_input_method_is_told_where_the_caret_is_drawn_and_not_where_it_was_measured",
        mutation: "report the caret's rectangle in the space it was measured in",
    },
];

/// Everything the two oracle gates run.
pub(crate) const ORACLES: &[Oracle] = &[
    Oracle {
        gate: "hits",
        phase: "hits",
        about: "what is under a point is the same in a window that has been running and in one \
                that computed the frame from nothing",
        guarded_by: HITS_GUARDS,
        sizes: &[
            Size {
                name: DOCUMENTS[0],
                required: HITS,
                skipped: 0,
            },
            Size {
                name: DOCUMENTS[1],
                required: HITS,
                skipped: 0,
            },
            Size {
                name: DOCUMENTS[2],
                required: HITS,
                skipped: 0,
            },
            Size {
                name: DOCUMENTS[3],
                required: HITS,
                skipped: 0,
            },
        ],
    },
    Oracle {
        gate: "a11y-geom",
        phase: "a11y-geom",
        about: "the rectangles handed to a screen reader and to an input method are the same in a \
                window that has been running and in one that computed the frame from nothing",
        guarded_by: GEOMETRY_GUARDS,
        sizes: &[
            Size {
                name: DOCUMENTS[0],
                required: GEOMETRY,
                skipped: 0,
            },
            Size {
                name: DOCUMENTS[1],
                required: GEOMETRY,
                skipped: 0,
            },
            Size {
                name: DOCUMENTS[2],
                required: GEOMETRY_AND_CARET,
                skipped: 0,
            },
            Size {
                name: DOCUMENTS[3],
                required: GEOMETRY_AND_CARET,
                skipped: 0,
            },
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::{DOCUMENTS, ORACLES};

    #[test]
    fn every_oracle_runs_over_four_documents_and_states_something_at_each() {
        for oracle in ORACLES {
            assert_eq!(
                oracle.sizes.len(),
                DOCUMENTS.len(),
                "{} runs over {} documents",
                oracle.gate,
                oracle.sizes.len(),
            );
            assert!(!oracle.about.is_empty(), "{} says nothing", oracle.gate);
            for size in oracle.sizes {
                assert!(
                    !size.required.is_empty(),
                    "{} at {} would be satisfied by a run that printed nothing",
                    oracle.gate,
                    size.name,
                );
            }
        }
    }

    #[test]
    fn the_smallest_document_is_among_them_and_so_is_the_shipped_one() {
        // Both ends, and for opposite reasons. The shell alone is where a comparison is likeliest
        // to be comparing nothing; the shipped gallery is the only one of these documents that has
        // transforms, sticky headers and overlays in it at all.
        assert!(DOCUMENTS.contains(&"s0"));
        assert!(DOCUMENTS.contains(&"s13"));
    }

    #[test]
    fn the_caret_half_is_claimed_only_where_there_is_something_to_type_into() {
        // The one thing this list must not acquire: a caret criterion at a size whose document has
        // no editable element, which would be satisfied by comparing nothing against nothing at
        // every step of the script.
        let geometry = ORACLES
            .iter()
            .find(|oracle| oracle.gate == "a11y-geom")
            .expect("the published-geometry oracle is registered");
        for size in geometry.sizes {
            let claims_caret = size
                .required
                .contains(&"caret_geometry_agrees_with_a_cold_window");
            assert_eq!(
                claims_caret,
                matches!(size.name, "s2" | "s13"),
                "{} claims the caret half: {claims_caret}",
                size.name,
            );
        }
    }
}
