//! What text is set in.
//!
//! Families, sizes, weights, line heights and letter spacing. Sizes carry their line height with
//! them — `--zui-type-size-md` and `--zui-type-leading-md` are meant to be used together — because
//! a size chosen without a line height is the single most reliable way to get text that looks
//! almost right.
//!
//! Sizes are absolute. An application that wants larger text everywhere overrides this group,
//! which is one declaration per step and leaves the spacing scale where it was — the two are moved
//! apart on purpose, because type that grew and gaps that grew with it is an interface that has
//! only zoomed.
//!
//! The two families name a preferred face and then fall back through the platform's own, so an
//! application that ships the face gets it and one that does not still gets a face rather than the
//! browser's default serif.

use crate::token::group::group;

group! {
    /// What text is set in.
    TypeTokens, prefix = "type", {
        /// The face everything is set in unless something says otherwise.
        family_sans => "family-sans", light = "Geist, ui-sans-serif, system-ui, sans-serif",
            dark = "Geist, ui-sans-serif, system-ui, sans-serif";
        /// The face for code, keyboard shortcuts and anything that has to line up in columns.
        family_mono => "family-mono",
            light = "\"Geist Mono\", ui-monospace, SFMono-Regular, Menlo, monospace",
            dark = "\"Geist Mono\", ui-monospace, SFMono-Regular, Menlo, monospace";

        /// Captions, badges, a tooltip, and the smallest legible text.
        size_xs => "size-xs", light = "12px", dark = "12px";
        /// The text on a control and in a menu: a button, an input, a table cell.
        size_sm => "size-sm", light = "14px", dark = "14px";
        /// Body text.
        size_md => "size-md", light = "16px", dark = "16px";
        /// A lead paragraph, or the text on a large control.
        size_lg => "size-lg", light = "18px", dark = "18px";
        /// A section heading.
        size_xl => "size-xl", light = "20px", dark = "20px";
        /// A page heading.
        size_x2l => "size-2xl", light = "24px", dark = "24px";
        /// A display heading.
        size_x3l => "size-3xl", light = "30px", dark = "30px";

        /// The line height for the smallest text.
        leading_xs => "leading-xs", light = "16px", dark = "16px";
        /// The line height for a control's text.
        leading_sm => "leading-sm", light = "20px", dark = "20px";
        /// The line height for body text.
        leading_md => "leading-md", light = "24px", dark = "24px";
        /// The line height for a lead paragraph.
        leading_lg => "leading-lg", light = "28px", dark = "28px";
        /// The line height for a section heading.
        leading_xl => "leading-xl", light = "28px", dark = "28px";
        /// The line height for a page heading.
        leading_x2l => "leading-2xl", light = "32px", dark = "32px";
        /// The line height for a display heading.
        leading_x3l => "leading-3xl", light = "36px", dark = "36px";

        /// Body text.
        weight_normal => "weight-normal", light = "400", dark = "400";
        /// A label, and a control's text.
        weight_medium => "weight-medium", light = "500", dark = "500";
        /// A heading.
        weight_semibold => "weight-semibold", light = "600", dark = "600";
        /// A display heading, or something being emphasised.
        weight_bold => "weight-bold", light = "700", dark = "700";

        /// Very large text, which needs pulling together hard.
        tracking_tighter => "tracking-tighter", light = "-0.05em", dark = "-0.05em";
        /// A heading, a card's title, and anything else set large.
        tracking_tight => "tracking-tight", light = "-0.025em", dark = "-0.025em";
        /// Ordinary text.
        tracking_normal => "tracking-normal", light = "0em", dark = "0em";
        /// Small text that has to stay legible.
        tracking_wide => "tracking-wide", light = "0.025em", dark = "0.025em";
        /// A small label set apart from what it labels.
        tracking_wider => "tracking-wider", light = "0.05em", dark = "0.05em";
        /// Small capitals, and the smallest labels of all.
        tracking_widest => "tracking-widest", light = "0.1em", dark = "0.1em";
    }
}

#[cfg(test)]
mod tests {
    use super::TypeTokens;

    /// The number of pixels in a length, for a value this module wrote.
    fn pixels(text: &str) -> f32 {
        text.trim_end_matches("px")
            .parse()
            .expect("every default size and line height is a plain pixel length")
    }

    #[test]
    fn every_size_has_a_line_height_under_the_same_name() {
        let sizes: Vec<String> = TypeTokens::PROPERTIES
            .iter()
            .filter_map(|name| name.strip_prefix("--zui-type-size-"))
            .map(str::to_owned)
            .collect();
        assert!(!sizes.is_empty());
        for step in sizes {
            let leading = format!("--zui-type-leading-{step}");
            assert!(
                TypeTokens::PROPERTIES.contains(&leading.as_str()),
                "{step} has a size and no line height"
            );
        }
    }

    #[test]
    fn a_line_height_is_always_taller_than_the_size_it_belongs_to() {
        let tokens = TypeTokens::light();
        let pairs = [
            (&tokens.size_xs, &tokens.leading_xs),
            (&tokens.size_sm, &tokens.leading_sm),
            (&tokens.size_md, &tokens.leading_md),
            (&tokens.size_lg, &tokens.leading_lg),
            (&tokens.size_xl, &tokens.leading_xl),
            (&tokens.size_x2l, &tokens.leading_x2l),
            (&tokens.size_x3l, &tokens.leading_x3l),
        ];
        for (size, leading) in pairs {
            assert!(
                pixels(leading) > pixels(size),
                "{leading} is not above {size}"
            );
        }
    }

    #[test]
    fn the_scale_only_ever_grows() {
        let tokens = TypeTokens::light();
        let ladder: Vec<f32> = [
            &tokens.size_xs,
            &tokens.size_sm,
            &tokens.size_md,
            &tokens.size_lg,
            &tokens.size_xl,
            &tokens.size_x2l,
            &tokens.size_x3l,
        ]
        .iter()
        .map(|step| pixels(step))
        .collect();
        assert!(
            ladder.windows(2).all(|pair| pair[0] < pair[1]),
            "{ladder:?} is not increasing"
        );
    }

    #[test]
    fn tracking_tightens_as_text_grows_and_loosens_as_it_shrinks() {
        // Large text needs pulling together and small text needs opening out, so the ladder runs
        // through zero rather than starting there.
        let tokens = TypeTokens::light();
        assert!(tokens.tracking_tighter.starts_with('-'));
        assert!(tokens.tracking_tight.starts_with('-'));
        assert_eq!(tokens.tracking_normal, "0em");
        assert!(!tokens.tracking_wide.starts_with('-'));
        assert!(!tokens.tracking_widest.starts_with('-'));
    }
}
