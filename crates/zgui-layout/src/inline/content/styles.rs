//! Lowering a cascaded style into text properties once per distinct style, not once per box.

use std::sync::Arc;

use rustc_hash::FxHashMap;
use zgui_css::{ComputedStyle, StructPtr};
use zgui_text::Brush;
use zgui_text_style::{ParagraphStyle, TextPaint, TextStyle, lower};

/// Everything one cascaded style contributes to a run of text.
#[derive(Clone, Debug)]
pub(crate) struct RunStyle {
    /// What the run shapes and breaks as, behind a shared pointer because a shaped result is
    /// cached against it and outlives the pass that produced it.
    pub(crate) text: Arc<TextStyle>,
    /// What the context around the run breaks as.
    pub(crate) paragraph: ParagraphStyle,
    /// The colour, and the identity a brush slot is claimed against.
    pub(crate) paint: TextPaint,
}

/// The three properties a cascaded style contributes, held against the identity of the groups they
/// came from.
///
/// The lowering itself is not repeated here — it has one home, and this calls it. What is held is
/// the answer, keyed on the *allocations* the cascade produced, so a paragraph of a thousand words
/// in one style lowers once and a document full of identically styled labels lowers once between
/// them all.
///
/// A miss means "not seen on these pointers", never "a different style": the cascade runs on
/// several threads and two of them can build equal groups separately. The cost of a miss is one
/// lowering.
#[derive(Debug, Default)]
pub(crate) struct TextStyles {
    /// The lowerings held.
    entries: FxHashMap<Key, RunStyle>,
    /// The brush slots claimed so far, against the same identities.
    brushes: FxHashMap<Key, Brush>,
}

/// The identity of the property groups a text lowering reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Key {
    /// The font group.
    font: StructPtr,
    /// The inherited-text group.
    text: StructPtr,
    /// The inherited-box group, which carries the writing direction.
    box_: StructPtr,
}

impl Key {
    /// The identity of one style.
    fn of(style: &ComputedStyle) -> Self {
        Self {
            font: StructPtr::font(style),
            text: StructPtr::inherited_text(style),
            box_: StructPtr::inherited_box(style),
        }
    }
}

impl TextStyles {
    /// The text properties of one cascaded style.
    pub(crate) fn get(&mut self, style: &ComputedStyle) -> RunStyle {
        let key = Key::of(style);
        if let Some(held) = self.entries.get(&key) {
            return held.clone();
        }
        let set = lower::style_set(style);
        let lowered = RunStyle {
            text: Arc::new(set.text),
            paragraph: set.paragraph,
            paint: set.paint,
        };
        self.entries.insert(key, lowered.clone());
        lowered
    }

    /// The brush slot one style's runs are drawn with, claiming one through `claim` if this style
    /// has not asked before.
    pub(crate) fn brush(
        &mut self,
        style: &ComputedStyle,
        paint: &TextPaint,
        claim: impl FnOnce(&TextPaint) -> Brush,
    ) -> Brush {
        let key = Key::of(style);
        if let Some(held) = self.brushes.get(&key) {
            return *held;
        }
        let brush = claim(paint);
        self.brushes.insert(key, brush);
        brush
    }

    /// How many distinct styles have been lowered.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}
