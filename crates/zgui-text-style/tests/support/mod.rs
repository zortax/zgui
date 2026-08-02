//! Building the style pairs the key tests compare.

// One module serving several test binaries: each of them uses the builders its own question needs,
// so anything unused here is unused *by that binary* rather than unused.
#![allow(dead_code)]

use zgui_css::{ComputedStyle, StyleDraft};
use zgui_text_style::{BreakingKey, ShapingKey, TextDamage, lower};

/// Two styles that agree on everything except one property, which they are guaranteed to disagree
/// on.
pub(crate) struct Vary {
    /// The style before the change.
    pub(crate) before: ComputedStyle,
    /// The style after it.
    pub(crate) after: ComputedStyle,
}

/// Builds a pair whose one varied property takes two *different* values from `choices`.
///
/// The two indices are drawn from a fixed integer hash rather than a random source, so a failing
/// case is reproducible from its index alone, and the second is offset from the first so that the
/// pair can never collapse onto one value — a pair that accidentally agreed would make an
/// assertion about "the key moved" pass or fail for the wrong reason.
pub(crate) fn pair<T: Copy>(index: u32, choices: &[T], apply: impl Fn(&mut StyleDraft, T)) -> Vary {
    assert!(choices.len() >= 2, "a pair needs two values to choose from");
    let first = seed(index) as usize % choices.len();
    let offset = 1 + seed(index ^ 0x5bf0_3635) as usize % (choices.len() - 1);
    let second = (first + offset) % choices.len();

    let mut before = StyleDraft::initial();
    apply(&mut before, choices[first]);
    let mut after = StyleDraft::initial();
    apply(&mut after, choices[second]);
    Vary {
        before: before.build(),
        after: after.build(),
    }
}

/// The initial style with exactly one property moved off its initial value.
///
/// Every question a caller can ask of it is asked of *all four* keys — the run's two and the
/// paragraph's two — because a property lowered onto the wrong one of the four would otherwise be
/// invisible: a shaping property hashed into the paragraph's breaking key still "moves a key", and
/// only naming which key catches that.
pub(crate) struct Varied {
    /// The style before the change.
    before: ComputedStyle,
    /// The style after it.
    after: ComputedStyle,
}

impl Varied {
    /// Builds the pair.
    pub(crate) fn of(apply: impl FnOnce(&mut StyleDraft)) -> Self {
        let before = StyleDraft::initial().build();
        let mut draft = StyleDraft::from_style(&before);
        apply(&mut draft);
        Self {
            before,
            after: draft.build(),
        }
    }

    /// The style before the change.
    pub(crate) fn before(&self) -> &ComputedStyle {
        &self.before
    }

    /// The style after it.
    pub(crate) fn after(&self) -> &ComputedStyle {
        &self.after
    }

    /// Asserts that the change costs a fresh shape, and says which key carried it.
    ///
    /// `in_paragraph` is which of the two shaping keys is expected to move — the run's or the
    /// paragraph's — and the *other* one is asserted not to move. A property that moved both would
    /// be lowered twice, and a change to it would be reported as damage by two independent routes.
    pub(crate) fn must_reshape(&self, in_paragraph: bool) {
        let run = (
            ShapingKey::of(&lower::text_style(&self.before)),
            ShapingKey::of(&lower::text_style(&self.after)),
        );
        let paragraph = (
            ShapingKey::of_paragraph(&lower::paragraph_style(&self.before)),
            ShapingKey::of_paragraph(&lower::paragraph_style(&self.after)),
        );
        if in_paragraph {
            assert_ne!(paragraph.0, paragraph.1, "the paragraph shaping key");
            assert_eq!(run.0, run.1, "the run shaping key must not move as well");
        } else {
            assert_ne!(run.0, run.1, "the run shaping key");
            assert_eq!(
                paragraph.0, paragraph.1,
                "the paragraph shaping key must not move as well",
            );
        }
        assert_eq!(
            TextDamage::between(&self.before, &self.after),
            TextDamage::Reshape,
        );
    }

    /// Asserts that the change costs a fresh break and no shape, and says which key carried it.
    ///
    /// The shaping assertions are the substance rather than the ceremony: a breaking property that
    /// leaked into either shaping key would still produce damage and still lay the text out
    /// correctly, at the cost of a shaping pass per restyle — a defect nothing but this notices.
    pub(crate) fn must_rebreak(&self, in_paragraph: bool) {
        assert_eq!(
            ShapingKey::of(&lower::text_style(&self.before)),
            ShapingKey::of(&lower::text_style(&self.after)),
            "a breaking-side property must not move the run shaping key",
        );
        assert_eq!(
            ShapingKey::of_paragraph(&lower::paragraph_style(&self.before)),
            ShapingKey::of_paragraph(&lower::paragraph_style(&self.after)),
            "a breaking-side property must not move the paragraph shaping key",
        );
        let run = (
            BreakingKey::of(&lower::text_style(&self.before)),
            BreakingKey::of(&lower::text_style(&self.after)),
        );
        let paragraph = (
            BreakingKey::of_paragraph(&lower::paragraph_style(&self.before)),
            BreakingKey::of_paragraph(&lower::paragraph_style(&self.after)),
        );
        if in_paragraph {
            assert_ne!(paragraph.0, paragraph.1, "the paragraph breaking key");
            assert_eq!(run.0, run.1, "the run breaking key must not move as well");
        } else {
            assert_ne!(run.0, run.1, "the run breaking key");
            assert_eq!(
                paragraph.0, paragraph.1,
                "the paragraph breaking key must not move as well",
            );
        }
        assert_eq!(
            TextDamage::between(&self.before, &self.after),
            TextDamage::Rebreak,
        );
    }
}

/// Which of the two keys of a kind a property is expected to move.
pub(crate) const RUN: bool = false;
/// Which of the two keys of a kind a property is expected to move.
pub(crate) const PARAGRAPH: bool = true;

/// A deterministic pseudo-random word, from the integer hash used for exactly this purpose.
pub(crate) fn seed(index: u32) -> u32 {
    let mut value = index.wrapping_mul(0x9e37_79b9);
    value ^= value >> 16;
    value = value.wrapping_mul(0x85eb_ca6b);
    value ^= value >> 13;
    value = value.wrapping_mul(0xc2b2_ae35);
    value ^ (value >> 16)
}
