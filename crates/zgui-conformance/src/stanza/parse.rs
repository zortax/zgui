//! Reading the engine's property definitions, and what each one implies about reachability.

use std::collections::BTreeMap;

use zgui_css::parity::AbsentReason;

use crate::stanza::locate::{LocateError, source_path};

/// The engine this framework builds.
const OURS: &str = "servo";

/// One property's definition, reduced to the three keys that decide whether it is reachable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stanza {
    /// The property, as a style sheet writes it.
    pub css_name: String,
    /// The single engine this property is built for, when it is built for only one.
    pub engine: Option<String>,
    /// The preference that has to be on before this build generates it.
    pub servo_pref: Option<String>,
    /// Which callers may use it: content, chrome, the user-agent sheet, or nothing.
    ///
    /// An empty string means the property is internal and no style sheet may ever set it, which is
    /// a different kind of unreachable from a preference that could be turned on.
    pub enabled_in: Option<String>,
}

impl Stanza {
    /// Whether this build does not generate the property because it belongs to another engine.
    pub fn is_other_engine_only(&self) -> bool {
        self.engine.as_deref().is_some_and(|engine| engine != OURS)
    }

    /// Whether the property is generated but switched off until something turns it on.
    pub fn is_gated(&self) -> bool {
        self.servo_pref.is_some()
            || self
                .enabled_in
                .as_deref()
                .is_some_and(|scope| scope != "content")
    }

    /// What the definitions say the reason for a property's absence would have to be.
    ///
    /// `None` means the definitions describe a property this build generates and exposes, so no
    /// absence reason can be right for it: whatever is missing is this framework's own doing.
    pub fn implied_absence(&self) -> Option<AbsentReason> {
        if self.is_other_engine_only() {
            return Some(AbsentReason::GeckoOnly);
        }
        self.is_gated().then_some(AbsentReason::PrefOff)
    }

    /// What gates the property, in words, for a report to print.
    pub fn gate(&self) -> Option<String> {
        match (&self.servo_pref, self.enabled_in.as_deref()) {
            (Some(pref), _) => Some(format!("preference `{pref}`")),
            (None, Some("")) => Some("internal: no style sheet may set it".to_owned()),
            (None, Some(scope)) => Some(format!("usable only from `{scope}`")),
            (None, None) => None,
        }
    }
}

/// Every property the engine defines, for every engine it can be built for.
#[derive(Clone, Debug, Default)]
pub struct Definitions {
    /// By property name.
    stanzas: BTreeMap<String, Stanza>,
}

impl Definitions {
    /// Reads the engine's definitions from wherever its sources are.
    ///
    /// # Errors
    ///
    /// Returns [`LocateError`] when the sources cannot be found, and a parse message when the file
    /// is not the table of tables it is supposed to be.
    pub fn load() -> Result<Self, String> {
        let path = source_path().map_err(|error: LocateError| error.to_string())?;
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        Self::parse(&text)
    }

    /// Reads definitions from text.
    ///
    /// # Errors
    ///
    /// Returns a message when the text is not a table of property tables.
    pub fn parse(text: &str) -> Result<Self, String> {
        let document: toml::Table = text.parse().map_err(|error| format!("{error}"))?;
        let stanzas = document
            .into_iter()
            .filter_map(|(css_name, value)| {
                let table = value.as_table()?;
                let text = |key: &str| {
                    table
                        .get(key)
                        .and_then(toml::Value::as_str)
                        .map(str::to_owned)
                };
                Some((
                    css_name.clone(),
                    Stanza {
                        css_name,
                        engine: text("engine"),
                        servo_pref: text("servo_pref"),
                        enabled_in: text("enabled_in"),
                    },
                ))
            })
            .collect();
        Ok(Self { stanzas })
    }

    /// One property's definition.
    pub fn get(&self, css_name: &str) -> Option<&Stanza> {
        self.stanzas.get(css_name)
    }

    /// How many properties are defined, across every engine.
    pub fn len(&self) -> usize {
        self.stanzas.len()
    }

    /// Whether nothing is defined, which is never true of a real file.
    pub fn is_empty(&self) -> bool {
        self.stanzas.is_empty()
    }

    /// Every property defined only for another engine, which is the set no census can see.
    pub fn other_engine_only(&self) -> Vec<&Stanza> {
        self.stanzas
            .values()
            .filter(|stanza| stanza.is_other_engine_only())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use zgui_css::parity::AbsentReason;

    use super::Definitions;

    /// The real definitions load, and they describe far more than this build generates.
    #[test]
    fn the_definitions_cover_more_than_this_build_generates() {
        let definitions = Definitions::load().expect("readable");
        assert!(definitions.len() > 400, "{}", definitions.len());
        assert!(definitions.other_engine_only().len() > 100);
        assert!(!definitions.is_empty());
    }

    /// The three shapes of unreachability are each derived from the definition rather than guessed.
    #[test]
    fn each_key_implies_the_reason_it_should() {
        let definitions = Definitions::load().expect("readable");

        let gecko_only = definitions.get("fill").expect("defined");
        assert_eq!(gecko_only.implied_absence(), Some(AbsentReason::GeckoOnly));

        let gated = definitions.get("counter-reset").expect("defined");
        assert_eq!(gated.implied_absence(), Some(AbsentReason::PrefOff));
        assert!(gated.gate().is_some());

        let ordinary = definitions.get("display").expect("defined");
        assert_eq!(ordinary.implied_absence(), None);
        assert_eq!(ordinary.gate(), None);
    }

    /// An internal property is gated by something other than a preference, and says so.
    #[test]
    fn an_internal_property_is_gated_without_a_preference() {
        let definitions = Definitions::load().expect("readable");
        let internal = definitions.get("-x-lang").expect("defined");
        assert_eq!(internal.servo_pref, None);
        assert!(internal.is_gated());
        assert_eq!(
            internal.gate().as_deref(),
            Some("internal: no style sheet may set it"),
        );
    }
}
