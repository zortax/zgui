//! The rule itself, over the text of one stored run.

/// What a duration's distribution field has to name.
const REQUIRED: [&str; 5] = ["p50=", "p95=", "p99=", "max=", "n="];

/// The units that make a measurement a duration.
const DURATIONS: [&str; 3] = ["us", "ms", "s"];

/// The field a `MEASURE` line's distribution sits in, counting from the scenario name.
const SPREAD_FIELD: usize = 9;

/// The field a `MEASURE` line's unit sits in.
const UNIT_FIELD: usize = 2;

/// One reason a stored run fails the gate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Violation {
    /// Which measurement or scenario it is about.
    pub(crate) subject: String,
    /// What is wrong with it.
    pub(crate) reason: String,
}

/// Every way `run` — the text of one stored `.tsv` — falls short.
///
/// Two rules, and both halves of each. A **duration** must carry a distribution naming all four
/// quantiles and the size of the population they were taken from; a median alone is a claim about
/// smoothness made from the only half of the evidence that cannot contradict it. A **scenario**
/// must publish how many of its frames were late against the interval it drove them at; a frame
/// that cost three milliseconds and arrived after its interval had elapsed is a dropped frame, and
/// no quantile of the frame's own cost can say so.
pub(crate) fn violations(run: &str) -> Vec<Violation> {
    let mut found = Vec::new();
    let mut scenarios: Vec<&str> = Vec::new();
    let mut paced: Vec<&str> = Vec::new();

    for line in run.lines() {
        if let Some(rest) = line.strip_prefix("PACE\t") {
            let mut field = rest.split('\t');
            let scenario = field.next().unwrap_or("");
            paced.push(scenario);
            let interval = field.next().unwrap_or("");
            if interval
                .parse::<f64>()
                .ok()
                .is_none_or(|value| value <= 0.0)
            {
                found.push(Violation {
                    subject: scenario.to_owned(),
                    reason: format!(
                        "its late-frame count is published against an interval of `{interval}`, \
                         which names no refresh at all"
                    ),
                });
            }
            continue;
        }
        let Some(rest) = line.strip_prefix("MEASURE\t") else {
            continue;
        };
        let fields: Vec<&str> = rest.split('\t').collect();
        let scenario = fields.first().copied().unwrap_or("");
        if !scenarios.contains(&scenario) {
            scenarios.push(scenario);
        }
        let name = fields.get(1).copied().unwrap_or("");
        let unit = fields.get(UNIT_FIELD).copied().unwrap_or("");
        if !DURATIONS.contains(&unit) {
            continue;
        }
        let spread = fields.get(SPREAD_FIELD).copied().unwrap_or("-");
        for wanted in REQUIRED {
            if !spread.contains(wanted) {
                found.push(Violation {
                    subject: format!("{scenario}.{name}"),
                    reason: format!(
                        "is a duration in `{unit}` and its distribution does not name `{}`: it \
                         published `{spread}`",
                        wanted.trim_end_matches('=')
                    ),
                });
            }
        }
    }

    for scenario in scenarios {
        if !paced.contains(&scenario) {
            found.push(Violation {
                subject: scenario.to_owned(),
                reason: "published no late-frame count against the interval it ran at".to_owned(),
            });
        }
    }
    found
}
