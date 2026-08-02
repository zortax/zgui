//! What the workloads gate runs, and what each of them has to still be asserting.

/// Where this list lives, for a failure that says what to edit.
pub(crate) const HERE: &str = "xtask/src/workloads/subject.rs";

/// One reference workload.
pub(crate) struct Workload {
    /// The binary inside `zgui-bench`.
    pub(crate) binary: &'static str,
    /// What it is a reference for, in one sentence.
    pub(crate) about: &'static str,
    /// The criteria it must still state, by the name it prints them under.
    ///
    /// Without this the gate is "run the binary": green when a criterion was deleted, green when
    /// one was renamed away by a change that meant to keep it, and green when the sweep that fed it
    /// stopped running. A workload that no longer makes the claim it exists for must fail rather
    /// than pass quietly.
    pub(crate) required: &'static [&'static str],
}

/// Every reference workload, in the order the gate runs them.
///
/// The 42-step gallery script is the fourth reference workload and is deliberately not here: it is
/// already a standing gate of its own, `cargo xtask ci → verify`, and running it twice under two
/// names would mean two places to relax it.
///
/// The non-virtualised large document is deliberately not here either, and that one is a decision
/// rather than an arrangement. It is a diagnostic probe — `cargo run --release -p zgui-bench --bin
/// unvirtualised-probe` — and its own module documentation says why: it is the only document in
/// this repository that would make the scroll phase's slope look important, so wiring it into `ci`
/// would let a phase justify itself against a document no application would ship. `glide-split`,
/// which splits that document's per-box cost between the walk's two duties, is out for the same
/// reason and for one more: it asks the walk to divide its descents, so what it times is not the
/// shape of the walk a frame makes.
pub(crate) const WORKLOADS: &[Workload] = &[
    Workload {
        binary: "static-slope",
        about: "a single-property update on one control of ten thousand costs what one control \
                costs, not what the document costs",
        required: &[
            "STATIC-locality",
            "STATIC-restyle-locality",
            "STATIC-visit-locality",
        ],
    },
    Workload {
        binary: "list-slope",
        about: "a hundred thousand rows under a fast wheel and under a touchpad cost what the port \
                costs, not what the data costs",
        required: &[
            "LIST-virtualisation-wheel",
            "LIST-virtualisation-touchpad",
            "LIST-damage-wheel",
            "LIST-damage-touchpad",
            "LIST-full-frames",
            "LIST-rebuilds-wheel",
            "LIST-rebuilds-touchpad",
            "LIST-glide",
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::WORKLOADS;

    #[test]
    fn every_workload_names_a_criterion_and_says_what_it_is_for() {
        for workload in WORKLOADS {
            assert!(
                !workload.required.is_empty(),
                "{} would be satisfied by a binary that printed nothing",
                workload.binary
            );
            assert!(
                !workload.about.is_empty(),
                "{} says nothing about why it is a reference",
                workload.binary
            );
        }
    }

    #[test]
    fn both_gestures_the_list_workload_is_named_for_are_covered() {
        // The list workload's whole point is that a wheel and a touchpad are two different frames
        // wearing one name. A list that quietly lost one of them would still run, still pass, and
        // still be called the hundred-thousand-row workload.
        let list = WORKLOADS
            .iter()
            .find(|workload| workload.binary == "list-slope")
            .expect("the list workload is registered");
        assert!(list.required.iter().any(|name| name.ends_with("-wheel")));
        assert!(list.required.iter().any(|name| name.ends_with("-touchpad")));
    }

    #[test]
    fn the_diagnostic_probe_is_not_one_of_them() {
        // The one thing this list must never acquire. See the doc comment above it.
        assert!(
            !WORKLOADS
                .iter()
                .any(|workload| workload.binary.contains("probe")),
            "the non-virtualised probe is a diagnostic and gating against it would let a phase \
             justify itself with a document nobody ships",
        );
    }

    #[test]
    fn neither_is_the_probe_that_splits_its_cost() {
        // Same document, same reason, and a second one: it drives the offsetting walk in a shape
        // no frame makes, so a criterion taken from it would gate the measurement rather than the
        // engine.
        assert!(
            !WORKLOADS
                .iter()
                .any(|workload| workload.binary == "glide-split"),
            "glide-split divides a walk that ships fused, over a document nobody ships; a gate \
             against it would hold the framework to how it was measured",
        );
    }
}
