//! The collected declarations, and the three questions a parity report asks of them.

use std::collections::BTreeMap;
use std::fmt;

use crate::parity::record::{ParityError, Registration};
use crate::parity::support::Support;

/// Every declaration gathered into one place, keyed by longhand.
///
/// The register is assembled rather than written: declarations live next to the code that reads
/// each property, and a reporting run collects them. What this type adds is the three questions
/// worth asking of the collection — how many properties are actually consumed, which declarations
/// have gone stale, and which longhands nobody has classified at all.
///
/// ```
/// use zgui_css::parity::{AbsentReason, Registration, Registry, Support};
///
/// let mut registry = Registry::new();
/// registry.insert(Registration::new("color", Support::Implemented("zgui-paint"))).unwrap();
/// registry.insert(Registration::new("quotes", Support::Ignored("no content lowering yet"))).unwrap();
///
/// assert_eq!(registry.counts().implemented, 1);
/// assert!(registry.check().is_empty(), "both declarations still match the engine");
/// assert_eq!(registry.unclassified(["color", "quotes", "opacity"]), vec!["opacity".to_owned()]);
/// ```
#[derive(Clone, Debug, Default)]
pub struct Registry {
    /// Declarations by longhand ident.
    entries: BTreeMap<&'static str, Registration>,
}

impl Registry {
    /// An empty register.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one declaration.
    ///
    /// # Errors
    ///
    /// Returns a [`Conflict`] when the property is already declared with a different treatment.
    /// Re-declaring it identically is not an error: the same property read by the same module from
    /// two files is one answer, not two.
    pub fn insert(&mut self, registration: Registration) -> Result<(), Conflict> {
        match self.entries.get(registration.ident()) {
            Some(existing) if existing.support() != registration.support() => Err(Conflict {
                css_name: registration.css_name(),
                existing: existing.support(),
                added: registration.support(),
            }),
            _ => {
                self.entries.insert(registration.ident(), registration);
                Ok(())
            }
        }
    }

    /// Adds a whole module's declarations.
    ///
    /// # Errors
    ///
    /// Returns the first [`Conflict`] encountered; rows before it are already recorded.
    pub fn extend(&mut self, registrations: &[Registration]) -> Result<(), Conflict> {
        for registration in registrations {
            self.insert(*registration)?;
        }
        Ok(())
    }

    /// Adds a whole module's declarations, keeping the strongest answer for each property.
    ///
    /// Two crates may each declare the same property for their own reasons, and both may be
    /// right — a property one crate only hashes and another reads is a property that is read. This
    /// keeps the stronger answer, as [`Support::strength`] orders them, and reports every property
    /// the two disagreed about so that a caller can show both.
    ///
    /// Prefer this to [`Registry::extend`] whenever the declarations come from more than one crate.
    /// `extend` reports a disagreement as an error and stops, which is what a caller assembling one
    /// module's rows wants and is exactly wrong for a caller assembling a whole framework's.
    ///
    /// ```
    /// use zgui_css::parity::{Registration, Registry, Support};
    ///
    /// let mut registry = Registry::new();
    /// let disagreements = registry.merge(&[
    ///     Registration::new("font_family", Support::Ignored("only hashed here")),
    ///     Registration::new("font_family", Support::Implemented("zgui-text-parley")),
    /// ]);
    ///
    /// assert_eq!(registry.counts().implemented, 1);
    /// assert_eq!(disagreements.len(), 1);
    /// assert_eq!(disagreements[0].kept, Support::Implemented("zgui-text-parley"));
    /// ```
    pub fn merge(&mut self, registrations: &[Registration]) -> Vec<Disagreement> {
        let mut disagreements = Vec::new();
        for registration in registrations {
            match self.entries.get(registration.ident()) {
                Some(existing) if existing.support() == registration.support() => {}
                Some(existing)
                    if existing.support().strength() == registration.support().strength() =>
                {
                    // Two notes for one treatment is one answer written twice, not a disagreement.
                    let _ = existing;
                }
                Some(existing) => {
                    let (kept, dropped) =
                        if registration.support().strength() > existing.support().strength() {
                            (*registration, *existing)
                        } else {
                            (*existing, *registration)
                        };
                    disagreements.push(Disagreement {
                        css_name: registration.css_name(),
                        kept: kept.support(),
                        dropped: dropped.support(),
                    });
                    self.entries.insert(kept.ident(), kept);
                }
                None => {
                    self.entries.insert(registration.ident(), *registration);
                }
            }
        }
        disagreements.sort_by(|left, right| left.css_name.cmp(&right.css_name));
        disagreements
    }

    /// The declaration for one longhand, named by its Rust spelling.
    pub fn get(&self, ident: &str) -> Option<Registration> {
        self.entries.get(ident).copied()
    }

    /// How many longhands are declared.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is declared.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every declaration, ordered by longhand.
    pub fn iter(&self) -> impl Iterator<Item = Registration> + '_ {
        self.entries.values().copied()
    }

    /// How many longhands fall into each treatment.
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for registration in self.entries.values() {
            match registration.support() {
                Support::Implemented(_) => counts.implemented += 1,
                Support::Ignored(_) => counts.ignored += 1,
                Support::Absent(_) => counts.absent += 1,
            }
        }
        counts
    }

    /// Every declaration the engine now contradicts.
    ///
    /// An empty result is the property that keeps the register honest: a preference flip, an engine
    /// upgrade or a patched build that changes what is reachable turns a row that was right into a
    /// failure here, rather than into a claim nobody re-checked.
    pub fn check(&self) -> Vec<ParityError> {
        self.entries
            .values()
            .filter_map(|registration| registration.check().err())
            .collect()
    }

    /// The longhands in `all` that nobody has declared, in the order they were offered.
    ///
    /// `all` is spelled the way a style sheet spells it. This is the question a parity gate asks:
    /// a property with no declaration is not a property that works, it is a property nobody has
    /// looked at, and the two are indistinguishable from the outside.
    pub fn unclassified<'a>(&self, all: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        all.into_iter()
            .filter(|css_name| {
                !self
                    .entries
                    .contains_key(css_name.replace('-', "_").as_str())
            })
            .map(str::to_owned)
            .collect()
    }
}

/// How many longhands fall into each treatment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Parsed, cascaded and read by some module.
    pub implemented: usize,
    /// Parsed and cascaded, deliberately unread.
    pub ignored: usize,
    /// Not available from the style engine at all.
    pub absent: usize,
}

impl Counts {
    /// Every declared longhand.
    pub const fn total(self) -> usize {
        self.implemented + self.ignored + self.absent
    }
}

/// One longhand declared twice, with two different answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// The name as a style sheet would write it.
    pub css_name: String,
    /// The treatment already recorded.
    pub existing: Support,
    /// The treatment offered afterwards.
    pub added: Support,
}

impl fmt::Display for Conflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is declared as {:?} and also as {:?}",
            self.css_name, self.existing, self.added
        )
    }
}

impl core::error::Error for Conflict {}

/// Two crates answering differently about one property, with the answer that was kept.
///
/// Not an error: both answers are usually true of the crate that made them, and which one
/// describes the framework is the stronger of the two. Reporting the other is what lets a parity
/// document show both rather than silently keeping one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Disagreement {
    /// The name as a style sheet would write it.
    pub css_name: String,
    /// The answer that was kept, which is the strongest one offered.
    pub kept: Support,
    /// The answer that was not.
    pub dropped: Support,
}
