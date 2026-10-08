//! Building a computed style directly, with no cascade behind it.
//!
//! Everything that reads computed styles has to be exercised against styles that differ in one
//! property at a time. Producing those from style sheets means running a cascade, which drags in a
//! device, a rule tree and a worker pool to answer a question about one longhand. A draft is the
//! short path: it starts from the initial value of every property and lets a caller overwrite the
//! ones the test is about.
//!
//! A draft is *not* a cascade. It performs no inheritance, resolves no relative units and applies
//! no rules, so the values written into it must already be computed ones.

use servo_arc::Arc as ServoArc;
use style::Atom;
use style::custom_properties::VariableValue;
use style::properties::{ComputedValues, ComputedValuesInner, style_structs};
use style::properties_and_values::value::ComputedValue as ComputedRegisteredValue;
use zgui_geom::CssPx;

use crate::computed::style::ComputedStyle;
use crate::values::font::FontSizeExt;

/// A computed style under construction.
///
/// ```
/// use zgui_css::StyleDraft;
/// use zgui_geom::CssPx;
///
/// use zgui_css::values::font::{FontSize, FontSizeExt};
///
/// let mut draft = StyleDraft::initial();
/// draft.font().font_size = FontSize::for_px(CssPx(24.0));
/// let style = draft.build();
///
/// assert_eq!(style.get_font().font_size.used_size().px(), 24.0);
/// ```
#[derive(Clone, Debug)]
pub struct StyleDraft {
    /// The values built so far.
    values: ComputedValues,
}

impl StyleDraft {
    /// A draft in which every property holds its initial value.
    pub fn initial() -> Self {
        let values = ComputedValues::initial_values_with_font_override(initial_font());
        Self {
            values: (*values).clone(),
        }
    }

    /// A draft starting from an existing style, so that a test can vary one property of it.
    pub fn from_style(style: &ComputedStyle) -> Self {
        Self {
            values: (**style).clone(),
        }
    }

    /// The box group, for writing.
    pub fn box_group(&mut self) -> &mut style_structs::Box {
        self.values.mutate_box()
    }

    /// The sizing, alignment and placement group, for writing.
    pub fn position_group(&mut self) -> &mut style_structs::Position {
        self.values.mutate_position()
    }

    /// The padding group, for writing.
    pub fn padding(&mut self) -> &mut style_structs::Padding {
        self.values.mutate_padding()
    }

    /// The list group, for writing.
    pub fn list(&mut self) -> &mut style_structs::List {
        self.values.mutate_list()
    }

    /// The font group, for writing.
    ///
    /// Family, weight, width and slant feed a digest the engine compares faces by, so
    /// [`StyleDraft::build`] recomputes it; a caller never has to.
    pub fn font(&mut self) -> &mut style_structs::Font {
        self.values.mutate_font()
    }

    /// The inherited-text group, for writing.
    pub fn inherited_text(&mut self) -> &mut style_structs::InheritedText {
        self.values.mutate_inherited_text()
    }

    /// The inherited-box group, for writing.
    pub fn inherited_box(&mut self) -> &mut style_structs::InheritedBox {
        self.values.mutate_inherited_box()
    }

    /// Sets `font-size`, which is the one property enough other things resolve against to be worth
    /// its own method.
    pub fn with_font_size(mut self, size: CssPx) -> Self {
        self.font().font_size = crate::values::font::FontSize::for_px(size);
        self
    }

    /// Declares the inheriting custom property `--name` with the value `value`, as written.
    ///
    /// # Panics
    ///
    /// Panics if `value` is no custom property value at all, such as an unbalanced bracket.
    pub fn with_custom_property(mut self, name: &str, value: &str) -> Self {
        let mut input = cssparser::ParserInput::new(value);
        let mut parser = cssparser::Parser::new(&mut input);
        let parsed = VariableValue::parse(&mut parser, None, &crate::values::custom::url_data())
            .expect("a custom property value");
        let mut custom = self.values.custom_properties().clone();
        custom.inherited.insert(
            &Atom::from(name),
            ComputedRegisteredValue::universal(servo_arc::Arc::new(parsed)),
        );
        let held: &ComputedValuesInner = &self.values;
        let values = ComputedValues::new(
            None,
            custom,
            Default::default(),
            held.writing_mode,
            held.effective_zoom,
            held.flags,
            None,
            None,
            held.clone_background(),
            held.clone_border(),
            held.clone_box(),
            held.clone_column(),
            held.clone_counters(),
            held.clone_effects(),
            held.clone_font(),
            held.clone_inherited_box(),
            held.clone_inherited_table(),
            held.clone_inherited_text(),
            held.clone_inherited_ui(),
            held.clone_list(),
            held.clone_margin(),
            held.clone_outline(),
            held.clone_padding(),
            held.clone_position(),
            held.clone_svg(),
            held.clone_table(),
            held.clone_text(),
            held.clone_ui(),
        );
        self.values = (*values).clone();
        self
    }

    /// Finishes the draft.
    pub fn build(mut self) -> ComputedStyle {
        self.values.mutate_font().compute_font_hash();
        ServoArc::new(self.values)
    }
}

impl Default for StyleDraft {
    fn default() -> Self {
        Self::initial()
    }
}

/// The initial font group, with its face digest already computed.
fn initial_font() -> style_structs::Font {
    let mut font = style_structs::Font::initial_values();
    font.compute_font_hash();
    font
}

#[cfg(test)]
mod tests {
    use super::StyleDraft;

    #[test]
    fn a_draft_declares_a_custom_property_a_reader_finds() {
        let style = StyleDraft::initial()
            .with_custom_property("syntax-keyword", "rgb(255, 0, 0)")
            .build();
        assert_eq!(
            crate::values::custom::text(&style, "syntax-keyword"),
            Some("rgb(255, 0, 0)")
        );
        assert!(crate::values::custom::color(&style, "syntax-keyword").is_some());
    }
}
