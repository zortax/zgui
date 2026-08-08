//! Which CSS longhands this crate lowers, and what happens to each of them afterwards.
//!
//! Lowering a property is not the same as honouring it. Some of the rows below reach a shaper and
//! change the glyphs it produces; others are read for one purpose only — moving a shaping or
//! breaking key, so that a change to them invalidates text that was laid out under the old value —
//! and nothing downstream acts on the value itself yet. Those two are different answers to "does
//! this property work", so they are declared differently: the first is
//! [`Support::Implemented`] and the second is
//! [`Support::Ignored`], whose note says what is still missing.
//!
//! Counting the second kind as implemented would be the whole failure this register exists to
//! prevent: an author who writes `writing-mode: vertical-rl` today gets horizontal text, and a
//! number that says otherwise is worse than no number.
//!
//! ```
//! use zgui_css::parity::Registry;
//!
//! // Against the engine as this framework configures it.
//! zgui_css::enable_css_features();
//!
//! let mut registry = Registry::new();
//! registry.extend(zgui_text_style::parity::REGISTERED).expect("no row declared twice");
//! assert_eq!(registry.counts().implemented, 27);
//! assert!(registry.check().is_empty(), "every row still matches what the engine says");
//! ```

use zgui_css::parity::{AbsentReason, Support};

/// Where the shaper is handed what the lowering produced.
const SHAPER: &str = "zgui-text-parley::shape::style";

/// Where the string a shaper is handed is built, which is where a property that changes *which*
/// characters are shaped has to be applied.
const GENERATOR: &str = "zgui-layout::inline::content::generate";

zgui_css::register_properties! {
    // The face: family, size and the axes it is selected and instanced along.
    font_family => Support::Implemented(SHAPER),
    font_size => Support::Implemented(SHAPER),
    font_weight => Support::Implemented(SHAPER),
    font_style => Support::Implemented(SHAPER),
    font_stretch => Support::Implemented(SHAPER),
    font_variation_settings => Support::Implemented(SHAPER),
    font_optical_sizing => Support::Implemented(SHAPER),
    line_height => Support::Implemented(SHAPER),

    // The face's optional substitutions, every one of which resolves into an OpenType feature.
    font_feature_settings => Support::Implemented(SHAPER),
    font_kerning => Support::Implemented(SHAPER),
    font_variant_ligatures => Support::Implemented(SHAPER),
    font_variant_caps => Support::Implemented(SHAPER),
    font_variant_position => Support::Implemented(SHAPER),
    font_variant_numeric => Support::Implemented(SHAPER),
    font_variant_east_asian => Support::Implemented(SHAPER),

    // The document language itself, which the lowering reads and no style sheet can write: the
    // engine sets it from the `lang` attribute and keeps the longhand switched off, so it is
    // consumed here and still unreachable from CSS. Both halves of that are true at once, and the
    // row records the half a parity count is about.
    _x_lang => Support::Absent(AbsentReason::PrefOff),

    // Spacing, wrapping and white space.
    letter_spacing => Support::Implemented(SHAPER),
    word_spacing => Support::Implemented(SHAPER),
    word_break => Support::Implemented(SHAPER),
    overflow_wrap => Support::Implemented(SHAPER),
    text_wrap_mode => Support::Implemented(SHAPER),

    // The two properties that decide which characters exist before anything is shaped. Both are
    // applied where the string a shaper is handed is generated, and both are in the shaping key —
    // which is what makes a change to either invalidate the string, the shaped paragraph and the
    // box that holds them.
    white_space_collapse => Support::Implemented(GENERATOR),
    text_transform => Support::Implemented(GENERATOR),
    tab_size => Support::Implemented(GENERATOR),

    // The paragraph.
    text_align => Support::Implemented(SHAPER),
    text_indent => Support::Implemented(SHAPER),
    direction => Support::Implemented(SHAPER),

    // The colour, which is lowered beside the two keys and never into them.
    color => Support::Implemented("zgui-text-style::lower::paint"),

    // Read only to move a key. Each of these changes glyphs or line positions in principle, so a
    // change to one must invalidate text laid out under the old value — but nothing downstream
    // acts on the value itself, so none of them is a property that works.
    font_synthesis_weight => Support::Ignored(
        "in the shaping key; the shaper offers no control over whether a weight is faked",
    ),
    writing_mode => Support::Ignored(
        "in the shaping key; there is no vertical inline formatting context to lay text out in",
    ),
    font_language_override => Support::Ignored(
        "in the shaping key as an OpenType language-system tag; nothing hands that tag to a shaper",
    ),
    line_break => Support::Ignored(
        "in the breaking key; the line breaker has no strictness control to hand it to",
    ),
    text_align_last => Support::Ignored(
        "in the breaking key; the aligner has no separate treatment for the final line",
    ),
    text_justify => Support::Ignored(
        "in the breaking key; a justified line is stretched one way and the keyword picks none",
    ),
}

#[cfg(test)]
mod tests {
    use zgui_css::parity::{Registry, Support};

    /// Every row is a claim about the engine, and the engine is asked whether it is still true.
    #[test]
    fn every_declaration_still_matches_what_the_engine_says() {
        zgui_css::enable_css_features();
        let mut registry = Registry::new();
        registry
            .extend(super::REGISTERED)
            .expect("no row is declared twice");
        assert_eq!(registry.len(), super::REGISTERED.len());
        assert!(
            registry.check().is_empty(),
            "a declaration here contradicts the engine as it is built: {:?}",
            registry.check(),
        );
    }

    /// The read-only-for-invalidation rows are declared as such, and nothing else is.
    ///
    /// Without this the honest half of the register would rot the first time one of them was
    /// promoted: a property whose consumer landed and whose row stayed `Ignored` under-counts, and
    /// a row flipped to `Implemented` with no consumer over-counts. Naming them makes either an
    /// edit to this list rather than a silent drift.
    #[test]
    fn only_the_properties_with_no_consumer_are_declared_unread() {
        let mut unread: Vec<String> = super::REGISTERED
            .iter()
            .filter(|row| matches!(row.support(), Support::Ignored(_)))
            .map(|row| row.css_name())
            .collect();
        unread.sort();
        assert_eq!(
            unread,
            [
                "font-language-override",
                "font-synthesis-weight",
                "line-break",
                "text-align-last",
                "text-justify",
                "writing-mode",
            ]
            .map(str::to_owned)
            .to_vec(),
        );
    }

    /// The claim the crate's parity number rests on is a count of properties something acts on.
    #[test]
    fn the_consumed_count_is_the_bulk_of_the_register() {
        let mut registry = Registry::new();
        registry.extend(super::REGISTERED).expect("no row twice");
        let counts = registry.counts();
        assert_eq!(
            counts.absent, 1,
            "`-x-lang` is the one row an author cannot reach: {counts:?}",
        );
        assert_eq!(
            counts.implemented, 27,
            "a row promoted to `Implemented` must be a deliberate edit to this number, because \
             the promotion is the claim a parity report repeats: {counts:?}",
        );
        assert_eq!(counts.ignored, 6, "{counts:?}");
        assert_eq!(counts.total(), super::REGISTERED.len());
    }
}
