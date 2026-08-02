//! The rows a probe cannot settle, and why each one cannot be settled here.
//!
//! A property claimed implemented whose probe changes nothing is an over-claim — unless the reason
//! the probe changes nothing is that the *instrument* cannot see the effect. That is true of a
//! whole family here: the shaper every deterministic test runs against has one face, no kerning, no
//! ligatures and no fallback, on purpose, because a suite written against real faces measures the
//! machine it runs on. Typography therefore has no observable consequence in this harness, and
//! saying so is honest where counting the rows as proven would not be.
//!
//! The list is short, explicit and read by the contradiction check, so an escape is a line someone
//! wrote and a reviewer can see — never a silence.

/// Why one property's effect cannot be observed here, if it cannot.
///
/// ```
/// use zgui_conformance::evidence::unproven;
///
/// assert!(unproven::reason("font-kerning").is_some());
/// assert_eq!(unproven::reason("width"), None);
/// ```
pub fn reason(css_name: &str) -> Option<&'static str> {
    ROWS.iter()
        .find(|(name, _)| *name == css_name)
        .map(|(_, reason)| *reason)
}

/// Every property whose consumer is real and whose effect this harness cannot see.
pub static ROWS: &[(&str, &str)] = &[
    // The face and its optional substitutions. All of these reach a shaper; the shaper the suite
    // runs against measures one fixed cluster width and applies no feature, so none of them can
    // move a glyph here.
    (
        "font-family",
        "the deterministic shaper has one face and never selects another",
    ),
    (
        "font-weight",
        "the deterministic shaper synthesises no weight",
    ),
    (
        "font-style",
        "the deterministic shaper synthesises no slant",
    ),
    ("font-stretch", "the deterministic shaper has no width axis"),
    (
        "font-variation-settings",
        "the deterministic shaper instances no axis",
    ),
    (
        "font-optical-sizing",
        "the deterministic shaper has no optical size axis",
    ),
    (
        "font-feature-settings",
        "the deterministic shaper applies no OpenType feature",
    ),
    (
        "font-kerning",
        "the deterministic shaper has no kerning to switch off",
    ),
    (
        "font-variant-ligatures",
        "the deterministic shaper forms no ligature",
    ),
    (
        "font-variant-caps",
        "the deterministic shaper has no small-capital coverage",
    ),
    (
        "font-variant-position",
        "the deterministic shaper has no superior or inferior forms",
    ),
    (
        "font-variant-numeric",
        "the deterministic shaper has one set of figures",
    ),
    (
        "font-variant-east-asian",
        "the deterministic shaper has no East Asian variants",
    ),
    (
        "font-language-override",
        "the deterministic shaper resolves no language system",
    ),
    // Line breaking. The deterministic shaper breaks at spaces and nowhere else, so a rule that
    // permits a break inside a word cannot be seen to take one.
    (
        "word-break",
        "the deterministic shaper breaks only at spaces",
    ),
    (
        "overflow-wrap",
        "the deterministic shaper breaks only at spaces",
    ),
    // The colour a run is drawn with is claimed against a brush slot in the scene's paint table,
    // which is not part of the fragment tree — the whole point of the slot being that a theme
    // change rewrites the table and leaves every shaped paragraph alone.
    (
        "color",
        "a run's colour is a brush slot in the scene's paint table, not a fragment field",
    ),
];

#[cfg(test)]
mod tests {
    use zgui_css::parity::catalog;

    use super::ROWS;

    /// Every escape names a longhand that exists, and names it once.
    ///
    /// A row for a property that is not generated would be an escape that can never be revisited,
    /// and it would sit in the list looking like diligence.
    #[test]
    fn every_escape_names_a_real_longhand_once() {
        zgui_css::enable_css_features();
        let generated = catalog::canonical_longhands();
        for (name, reason) in ROWS {
            assert!(generated.contains(&(*name).to_owned()), "`{name}`");
            assert!(!reason.is_empty(), "`{name}` escapes without a reason");
            assert_eq!(
                ROWS.iter().filter(|(other, _)| other == name).count(),
                1,
                "`{name}` is listed twice",
            );
        }
    }

    /// The escapes are a small minority of what is claimed, and not a way around the check.
    #[test]
    fn the_escape_list_is_small() {
        assert!(ROWS.len() < 20, "{}", ROWS.len());
    }
}
