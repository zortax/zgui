//! A theme, as the text of a style sheet.
//!
//! The tokens reach the document as a style sheet rather than as inline properties on an element,
//! and that is the decision the whole design rests on. A sheet is at the author origin, so an
//! application overrides any token by writing an ordinary rule of its own; inline properties would
//! beat every such rule and the library would be un-themeable except by rebuilding it.
//!
//! It also makes `System` free. `prefers-color-scheme` is a media query the style engine already
//! evaluates against the surface, so writing the dark tokens inside one means the desktop's own
//! setting decides — with no portal call, no signal, and nothing to keep in step.

use crate::scheme::ColorScheme;
use crate::theme::Theme;
use crate::token::Declarations;

/// The name the theme's own sheet is installed under.
///
/// Installing under a name replaces what was there and keeps its place in the cascade, so this is
/// what makes a theme change one sheet replacement rather than a sheet that moves to the front of
/// the author origin and quietly starts beating rules it used to lose to.
pub const THEME_SHEET: &str = "zgui-ui-theme";

/// Writes `light` and `dark` out as a style sheet, for the scheme `scheme` asks for.
///
/// `selector` is what the tokens are declared on — `:root` for a whole window, a class for a
/// region that is themed differently from the rest.
///
/// | Scheme | What is written |
/// |---|---|
/// | [`ColorScheme::Light`] | one rule, the light tokens |
/// | [`ColorScheme::Dark`] | one rule, the dark tokens |
/// | [`ColorScheme::System`] | the light tokens, then the dark ones inside `@media (prefers-color-scheme: dark)` |
///
/// ```
/// use zgui_ui_tokens::{ColorScheme, Theme, theme_sheet};
///
/// let css = theme_sheet(":root", &Theme::light(), &Theme::dark(), ColorScheme::System);
/// assert!(css.starts_with(":root {"));
/// assert!(css.contains("@media (prefers-color-scheme: dark)"));
///
/// // Pinned to one scheme, the media query is not written at all — so nothing in the sheet can
/// // change under the interface when the desktop's setting does.
/// let pinned = theme_sheet(":root", &Theme::light(), &Theme::dark(), ColorScheme::Dark);
/// assert!(!pinned.contains("@media"));
/// ```
pub fn theme_sheet(selector: &str, light: &Theme, dark: &Theme, scheme: ColorScheme) -> String {
    let mut css = String::new();

    if scheme.wants_light() {
        let mut declarations = Declarations::new();
        light.declare(&mut declarations);
        css.push_str(&declarations.into_rule(selector));
    }

    if scheme.wants_dark() {
        let mut declarations = Declarations::new();
        dark.declare(&mut declarations);
        let rule = declarations.into_rule(selector);
        if rule.is_empty() {
            return css;
        }
        if !css.is_empty() {
            css.push('\n');
        }
        // Pinned to dark, the tokens are unconditional. Deferred to the desktop, they are the
        // override the media query switches on — and the light ones above are what is in force
        // when it does not match.
        if scheme == ColorScheme::System {
            css.push_str("@media (prefers-color-scheme: dark) { ");
            css.push_str(&rule);
            css.push_str(" }");
        } else {
            css.push_str(&rule);
        }
    }

    css
}

#[cfg(test)]
mod tests {
    use super::{THEME_SHEET, theme_sheet};
    use crate::scheme::ColorScheme;
    use crate::theme::Theme;

    fn sheet(scheme: ColorScheme) -> String {
        theme_sheet(":root", &Theme::light(), &Theme::dark(), scheme)
    }

    #[test]
    fn a_pinned_scheme_writes_one_rule_and_no_media_query() {
        for scheme in [ColorScheme::Light, ColorScheme::Dark] {
            let css = sheet(scheme);
            assert!(!css.contains("@media"), "{scheme:?} wrote a media query");
            assert_eq!(css.matches(":root {").count(), 1);
        }
    }

    #[test]
    fn the_two_pinned_schemes_write_different_values_for_the_same_properties() {
        let light = sheet(ColorScheme::Light);
        let dark = sheet(ColorScheme::Dark);
        assert_ne!(light, dark);
        for name in Theme::properties() {
            assert!(
                light.contains(name),
                "{name} is missing from the light sheet"
            );
            assert!(dark.contains(name), "{name} is missing from the dark sheet");
        }
    }

    #[test]
    fn deferring_to_the_desktop_writes_the_light_tokens_first_and_the_dark_ones_behind_the_query() {
        let css = sheet(ColorScheme::System);
        let query = css
            .find("@media (prefers-color-scheme: dark)")
            .expect("the query is written");
        let light_background = css
            .find("--zui-scale-neutral-1: #fcfcfd")
            .expect("the light ramp is written");
        let dark_background = css
            .find("--zui-scale-neutral-1: #111113")
            .expect("the dark ramp is written");
        assert!(
            light_background < query && query < dark_background,
            "the dark tokens must sit inside the query, after the light ones"
        );
        // Balanced: the rule's braces plus the query's own.
        assert_eq!(
            css.matches('{').count(),
            css.matches('}').count(),
            "the sheet does not close what it opens"
        );
    }

    #[test]
    fn a_scoped_selector_themes_a_region_rather_than_the_window() {
        let css = theme_sheet(
            ".panel",
            &Theme::light(),
            &Theme::dark(),
            ColorScheme::Light,
        );
        assert!(css.starts_with(".panel {"));
        assert!(!css.contains(":root"));
    }

    #[test]
    fn the_sheet_has_one_name_so_that_replacing_it_keeps_its_place_in_the_cascade() {
        assert_eq!(THEME_SHEET, "zgui-ui-theme");
    }
}
