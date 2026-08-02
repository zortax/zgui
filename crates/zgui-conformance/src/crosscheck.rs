//! Whether a register row's stated reason and the engine's own definition agree.
//!
//! A row saying a property is unavailable also says *why*, and the why decides who fixes it and
//! what it costs: a preference to flip, a build to patch, or a definition that does not exist at
//! all. Those are not editorial distinctions and they are not guessable, which is why every one of
//! them has been got wrong at least once — a whole property group recorded as *absent from the
//! engine* when the engine defines all of it and simply builds it for someone else.
//!
//! The definitions answer all three, so a wrong reason is a build failure rather than prose nobody
//! re-read.
//!
//! ```
//! use zgui_conformance::crosscheck;
//! use zgui_conformance::stanza::Definitions;
//! use zgui_css::parity::{AbsentReason, Registration, Support};
//!
//! let definitions = Definitions::load().expect("the engine's definitions are readable");
//!
//! // `fill` is defined, for another engine. Calling that "no definition exists" is wrong.
//! let row = Registration::new("fill", Support::Absent(AbsentReason::NotInStylo));
//! assert_eq!(crosscheck::check(&[row], &definitions).len(), 1);
//! ```

use zgui_css::parity::{AbsentReason, Registration, Support};

use crate::stanza::Definitions;

/// A row whose stated reason the engine's definitions contradict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    /// The property, as a style sheet writes it.
    pub css_name: String,
    /// The reason the row states.
    pub stated: AbsentReason,
    /// The reason the definitions imply, or nothing when they imply the property is reachable.
    pub implied: Option<AbsentReason>,
    /// What the definitions actually say, for the failure message.
    pub definition: String,
}

impl core::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "`{}` is recorded as {:?} but the engine's definition implies {:?} ({})",
            self.css_name, self.stated, self.implied, self.definition,
        )
    }
}

/// Every row whose stated reason disagrees with the engine's own definition.
///
/// Only rows that state a reason are checked, and only the two reasons the definitions can settle:
/// which engine a property is built for, and whether it is gated. The other reasons are statements
/// about this framework rather than about the engine, and a file of engine definitions has nothing
/// to say about them.
pub fn check(rows: &[Registration], definitions: &Definitions) -> Vec<Mismatch> {
    let mut out = Vec::new();
    for row in rows {
        let Support::Absent(stated) = row.support() else {
            continue;
        };
        let css_name = row.css_name();
        let Some(stanza) = definitions.get(&css_name) else {
            // No definition at all is the one reason a missing definition proves.
            if stated != AbsentReason::NotInStylo {
                out.push(Mismatch {
                    css_name,
                    stated,
                    implied: Some(AbsentReason::NotInStylo),
                    definition: "no definition of this property exists".to_owned(),
                });
            }
            continue;
        };
        let implied = stanza.implied_absence();
        let settled = matches!(
            stated,
            AbsentReason::GeckoOnly | AbsentReason::NotInStylo | AbsentReason::PrefOff
        );
        if settled && implied != Some(stated) {
            out.push(Mismatch {
                css_name,
                stated,
                implied,
                definition: describe(stanza),
            });
        }
    }
    out
}

/// What a definition says, in one line, for a failure message.
fn describe(stanza: &crate::stanza::Stanza) -> String {
    match (&stanza.engine, stanza.gate()) {
        (Some(engine), Some(gate)) => format!("built for `{engine}` only, and {gate}"),
        (Some(engine), None) => format!("built for `{engine}` only"),
        (None, Some(gate)) => format!("built for every engine, {gate}"),
        (None, None) => "built for every engine and exposed to content".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::parity::{AbsentReason, Registration, Support};

    use crate::stanza::Definitions;

    use super::check;

    /// Every absence reason recorded anywhere in the workspace matches the engine's definitions.
    #[test]
    fn no_row_in_the_workspace_states_the_wrong_reason() {
        let definitions = Definitions::load().expect("the engine's definitions are readable");
        let mismatches = check(&crate::registrations(), &definitions);
        assert!(
            mismatches.is_empty(),
            "{}",
            mismatches
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    /// A deliberately mislabelled row is caught, in both directions that matter.
    ///
    /// Without this the check above would pass just as well if it were reading an empty set of
    /// definitions or comparing nothing — and "the cross-check is green" would mean nothing at all.
    #[test]
    fn a_mislabelled_row_is_caught() {
        let definitions = Definitions::load().expect("the engine's definitions are readable");

        // Built for another engine, recorded as though no definition existed.
        let as_missing = Registration::new("fill", Support::Absent(AbsentReason::NotInStylo));
        // Gated behind a preference, recorded as though it were another engine's.
        let as_other_engine =
            Registration::new("counter_reset", Support::Absent(AbsentReason::GeckoOnly));
        // Reachable, recorded as gated.
        let as_gated = Registration::new("display", Support::Absent(AbsentReason::PrefOff));

        let found = check(&[as_missing, as_other_engine, as_gated], &definitions);
        assert_eq!(found.len(), 3, "{found:#?}");
        assert_eq!(found[0].implied, Some(AbsentReason::GeckoOnly));
        assert_eq!(found[1].implied, Some(AbsentReason::PrefOff));
        assert_eq!(found[2].implied, None);
    }

    /// A correctly labelled row is not caught, which is the other half of the control.
    #[test]
    fn a_correct_row_is_not_caught() {
        let definitions = Definitions::load().expect("the engine's definitions are readable");
        let right = Registration::new("fill", Support::Absent(AbsentReason::GeckoOnly));
        assert_eq!(check(&[right], &definitions), Vec::new());
    }
}
