//! Custom properties, read off a computed style.
//!
//! A custom property's computed value is a token stream rather than a typed value, which is what
//! makes it the non-forking way to feed this engine something it has no property for. Reading one
//! back is therefore reading text and parsing it as whatever the reader expects — a colour, a
//! length — with the reader deciding what an unparsable value means.

use style::custom_properties::Name;

use crate::computed::style::ComputedStyle;

/// The computed text of the custom property `name` on `style`, without its `--` prefix.
///
/// Looks in the inherited map first and the non-inherited one after, which is the order a lookup
/// resolves in: a property declared on an ancestor is inherited unless this element declared one
/// of its own, and an element's own declaration lands in whichever map its registration says.
///
/// `None` means nothing declared it anywhere up the tree.
pub fn text<'a>(style: &'a ComputedStyle, name: &str) -> Option<&'a str> {
    let name = Name::from(name);
    let properties = style.custom_properties();
    properties
        .inherited
        .get(&name)
        .or_else(|| properties.non_inherited.get(&name))
        .and_then(|value| value.as_universal())
        .map(|value| value.css.as_str())
}

/// The custom property `name` on `style`, read as a colour.
///
/// `currentColor` — which is what an unset drawing paint means — resolves against the element's own
/// `color`, so the keyword works here for the same reason it works anywhere else. `None` means the
/// property was not declared, or its text is not a colour, and the caller decides which default
/// that falls back to.
pub fn color(style: &ComputedStyle, name: &str) -> Option<zgui_color::Color> {
    let text = text(style, name)?.trim();
    if text.eq_ignore_ascii_case("none") {
        return None;
    }
    if text.eq_ignore_ascii_case("currentcolor") {
        return Some(crate::values::color::to_color(
            crate::values::color::current(style),
        ));
    }
    let absolute = parse_color(text)?.resolve_to_absolute(None).ok()?;
    Some(crate::values::color::to_color(&absolute))
}

/// Every custom property declared on `style`, without its `--` prefix.
///
/// The inherited map first and the non-inherited one after, which is the order a lookup resolves
/// in. Walked by index rather than by iterator because that is what the engine exposes, and a
/// style with no custom properties at all costs one call.
///
/// This is for a reader that has to *discover* which properties were written — an effect's
/// parameters, say, whose names belong to the effect rather than to this engine. A reader that
/// knows the name it wants asks for it with [`text`] instead.
pub fn names(style: &ComputedStyle) -> Vec<String> {
    let properties = style.custom_properties();
    let mut found = Vec::new();
    let mut index = 0;
    while let Some((name, _)) = properties.property_at(index) {
        found.push(name.as_ref().to_owned());
        index += 1;
    }
    found
}

/// The custom property `name` on `style`, read as a plain number.
///
/// A number with no unit, which is what a parameter to something outside this engine usually is: a
/// superellipse's exponent, a strength from zero to one, a count. `None` means the property was not
/// declared, or its text is not a number.
pub fn number(style: &ComputedStyle, name: &str) -> Option<f32> {
    text(style, name)?.trim().parse().ok()
}

/// The custom property `name` on `style`, read as a length in CSS pixels.
///
/// Only absolute lengths resolve: a font-relative unit would need the element's font, which is a
/// second input and a second thing to invalidate on, and a stroke width is not a place that needs
/// one.
pub fn length(style: &ComputedStyle, name: &str) -> Option<f32> {
    parse_length(text(style, name)?)
}

/// Reads one absolute length.
fn parse_length(text: &str) -> Option<f32> {
    let text = text.trim();
    let (number, unit) = text.split_at(text.find(|c: char| c.is_alphabetic() || c == '%')?);
    let number: f32 = number.trim().parse().ok()?;
    match unit.trim() {
        "px" => Some(number),
        "in" => Some(number * 96.0),
        "cm" => Some(number * 96.0 / 2.54),
        "mm" => Some(number * 96.0 / 25.4),
        "pt" => Some(number * 96.0 / 72.0),
        "pc" => Some(number * 16.0),
        _ => None,
    }
}

/// Parses one colour out of a token stream.
///
/// Spelled out as a declaration and parsed as one, because the entry point that takes a value and
/// a property is the one this build exposes: it needs no parser context of its own, so nothing
/// here has to name a type the dependency ledger keeps out of reach.
fn parse_color(text: &str) -> Option<style::values::specified::Color> {
    use style::context::QuirksMode;
    use style::properties::{PropertyDeclaration, parse_style_attribute};
    use style::stylesheets::CssRuleType;

    let block = parse_style_attribute(
        &format!("color:{text}"),
        &url_data(),
        None,
        QuirksMode::NoQuirks,
        CssRuleType::Style,
    );
    match block.declarations().first() {
        Some(PropertyDeclaration::Color(color)) => Some(color.0.clone()),
        _ => None,
    }
}

/// The base a colour's URL-valued components would resolve against, of which a colour has none.
fn url_data() -> style::stylesheets::UrlExtraData {
    use core::str::FromStr;
    fn parsed<T: FromStr>(text: &str) -> T
    where
        T::Err: core::fmt::Debug,
    {
        text.parse().expect("a well-formed base URL")
    }
    style::stylesheets::UrlExtraData(servo_arc::Arc::new(parsed("zgui:///")))
}

#[cfg(test)]
mod tests {
    use crate::StyleDraft;

    use super::{color, length, text};

    #[test]
    fn nothing_is_declared_on_an_initial_style() {
        let style = StyleDraft::initial().build();
        assert_eq!(text(&style, "zgui-fill"), None);
        assert_eq!(color(&style, "zgui-fill"), None);
        assert_eq!(length(&style, "zgui-stroke-width"), None);
    }

    #[test]
    fn a_length_resolves_only_for_absolute_units() {
        assert_eq!(super::parse_length("2px"), Some(2.0));
        assert_eq!(super::parse_length("1in"), Some(96.0));
        assert_eq!(super::parse_length("1pt"), Some(96.0 / 72.0));
        assert_eq!(super::parse_length("2em"), None);
        assert_eq!(super::parse_length("2"), None);
        assert_eq!(super::parse_length("wide"), None);
    }
}
