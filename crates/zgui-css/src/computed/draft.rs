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
use style::properties::{ComputedValues, style_structs};
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
