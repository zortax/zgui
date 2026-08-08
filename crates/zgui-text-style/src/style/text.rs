//! The style of one run of text.

use smallvec::SmallVec;
use zgui_geom::CssPx;
use zgui_interned::Ident;

use crate::key::digest::Digest;
use crate::style::face::{FontFeature, FontSlant, FontVariation};
use crate::style::family::{FontFamilyList, GenericFamily};
use crate::style::line_height::LineHeight;
use crate::style::optical::{OPTICAL_SIZE_AXIS, OpticalSizing};
use crate::style::spacing::LengthPercent;
use crate::style::synthesis::SynthesisWeight;
use crate::style::transform::TextTransform;
use crate::style::variant::FontVariant;
use crate::style::wrap::{LineBreak, OverflowWrap, WhiteSpaceCollapse, WordBreak, WrapMode};

/// Everything about one run of text except its colour.
///
/// The type is split down the middle on purpose. The fields above [`TextStyle::overflow_wrap`]
/// decide which glyphs exist and how wide they are; the three below decide only where the lines
/// fall. [`hash_shaping`](TextStyle::hash_shaping) and [`hash_breaking`](TextStyle::hash_breaking)
/// hash the halves separately, and every consumer of the split reads them rather than the fields,
/// so there is one place a property can be on the wrong side of the line.
///
/// The colour is absent, and that absence is load-bearing rather than an omission: a run's paint is
/// an index into a table the shaped result does not own, so re-theming a document rewrites table
/// entries and leaves every shaped paragraph valid. Putting a colour in here would make a
/// dark-mode toggle re-shape every string in the application.
///
/// # What a shaper is handed, and what is only stored
///
/// Two of the fields are not passed on as they stand. [`TextStyle::variant`] is six properties that
/// each turn one of the face's optional substitutions on or off, and [`TextStyle::optical_sizing`]
/// moves a variable-font axis; both are resolved into the vocabulary a shaper already takes, by
/// [`shaping_features`](TextStyle::shaping_features) and
/// [`shaping_variations`](TextStyle::shaping_variations). A consumer calls those two rather than
/// reading [`TextStyle::features`] and [`TextStyle::variations`] directly, which is what stops a
/// property from being lowered and then quietly dropped on the way to the shaper.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    /// `font-family`, in author order.
    pub family: FontFamilyList,
    /// The size glyphs are drawn at.
    pub size: CssPx,
    /// `font-weight`, between 1 and 1000.
    pub weight: f32,
    /// `font-style`.
    pub slant: FontSlant,
    /// `font-width` as a fraction of normal, where `1.0` is `normal`.
    pub width: f32,
    /// `font-synthesis-weight`.
    pub synthesis_weight: SynthesisWeight,
    /// `font-optical-sizing`.
    pub optical_sizing: OpticalSizing,
    /// `font-variation-settings`, in author order.
    pub variations: SmallVec<[FontVariation; 2]>,
    /// `font-feature-settings`, in author order.
    pub features: SmallVec<[FontFeature; 2]>,
    /// `font-kerning` and the five `font-variant-*` longhands.
    pub variant: FontVariant,
    /// The language the text is in, which selects a face's locale-specific forms. Absent when the
    /// document declared none.
    pub language: Option<Ident>,
    /// The OpenType language system to shape with, when the document forced one that is not the
    /// one [`TextStyle::language`] would have selected. Packed big-endian, as tags are.
    pub language_system: Option<u32>,
    /// `letter-spacing`, extra advance after every cluster. A percentage is of the font size.
    pub letter_spacing: LengthPercent,
    /// `word-spacing`, extra advance on every space cluster. A percentage is of the advance of a
    /// space in the face that was chosen.
    pub word_spacing: LengthPercent,
    /// `line-height`.
    pub line_height: LineHeight,
    /// `word-break`.
    pub word_break: WordBreak,
    /// `white-space-collapse`, which decides what text is shaped at all.
    pub white_space: WhiteSpaceCollapse,
    /// `text-transform`, which decides which characters are shaped at all.
    ///
    /// Applied where the string a shaper is handed is generated, before this style ever reaches a
    /// shaper — so nothing below reads it. It is carried here for one reason, and the reason is the
    /// only thing keeping the feature correct: the shaping key is hashed from these fields, and a
    /// transform absent from the key is a transform whose change never invalidates the generated
    /// string, the shaped paragraph or the box that holds them.
    pub transform: TextTransform,
    /// `overflow-wrap` — the first of the three breaking-side properties.
    pub overflow_wrap: OverflowWrap,
    /// `text-wrap-mode`.
    pub wrap_mode: WrapMode,
    /// `line-break`.
    pub line_break: LineBreak,
}

impl TextStyle {
    /// The style a document with no rules at all resolves to.
    pub fn initial() -> Self {
        Self {
            family: FontFamilyList::generic(GenericFamily::Serif),
            size: CssPx(16.0),
            weight: 400.0,
            slant: FontSlant::Upright,
            width: 1.0,
            synthesis_weight: SynthesisWeight::Auto,
            optical_sizing: OpticalSizing::Auto,
            variations: SmallVec::new(),
            features: SmallVec::new(),
            variant: FontVariant::initial(),
            language: None,
            language_system: None,
            letter_spacing: LengthPercent::ZERO,
            word_spacing: LengthPercent::ZERO,
            line_height: LineHeight::Normal,
            word_break: WordBreak::Normal,
            white_space: WhiteSpaceCollapse::Collapse,
            transform: TextTransform::none(),
            overflow_wrap: OverflowWrap::Normal,
            wrap_mode: WrapMode::Wrap,
            line_break: LineBreak::Auto,
        }
    }

    /// The full OpenType feature list a shaper is handed for this run.
    ///
    /// The features the `font-variant-*` family and `font-kerning` ask for, then the ones the
    /// author wrote in `font-feature-settings` — and a derived feature is dropped when the author
    /// named the same tag. That is the priority CSS gives the two: a hand-written
    /// `font-feature-settings` outranks anything a higher-level property implies. Deciding it here
    /// rather than by list order means the answer does not depend on which of two entries for one
    /// tag a particular shaper happens to honour.
    ///
    /// ```
    /// use zgui_text_style::{FontFeature, TextStyle, tag, variant};
    ///
    /// let mut style = TextStyle::initial();
    /// style.variant.kerning = variant::FontKerning::None;
    /// assert_eq!(style.shaping_features()[0].value, 0, "the property asks for kern=0");
    ///
    /// // The author's own setting for the same tag wins outright.
    /// style.features.push(FontFeature { tag: tag(b"kern"), value: 1 });
    /// assert_eq!(style.shaping_features().len(), 1);
    /// assert_eq!(style.shaping_features()[0].value, 1);
    /// ```
    pub fn shaping_features(&self) -> SmallVec<[FontFeature; 4]> {
        let mut derived = self.variant.features();
        derived.retain(|feature| !self.features.iter().any(|own| own.tag == feature.tag));
        derived.extend(self.features.iter().copied());
        derived
    }

    /// The full variable-axis list a shaper is handed for this run.
    ///
    /// `font-optical-sizing: auto` drives the `opsz` axis from the font size, and the author's own
    /// `font-variation-settings` outranks it for the same reason and by the same rule as in
    /// [`shaping_features`](TextStyle::shaping_features).
    ///
    /// ```
    /// use zgui_text_style::{OpticalSizing, TextStyle, tag};
    ///
    /// let style = TextStyle::initial();
    /// assert_eq!(style.shaping_variations()[0].tag, tag(b"opsz"));
    /// assert_eq!(style.shaping_variations()[0].value, 16.0, "the font size drives the axis");
    ///
    /// let mut fixed = TextStyle::initial();
    /// fixed.optical_sizing = OpticalSizing::None;
    /// assert!(fixed.shaping_variations().is_empty());
    /// ```
    pub fn shaping_variations(&self) -> SmallVec<[FontVariation; 4]> {
        let mut resolved = SmallVec::new();
        let author_set_it = self
            .variations
            .iter()
            .any(|variation| variation.tag == OPTICAL_SIZE_AXIS);
        if self.optical_sizing == OpticalSizing::Auto && !author_set_it {
            resolved.push(FontVariation {
                tag: OPTICAL_SIZE_AXIS,
                value: self.size.0,
            });
        }
        resolved.extend(self.variations.iter().copied());
        resolved
    }

    /// Mixes the shaping half into a digest.
    pub fn hash_shaping(&self, digest: &mut Digest) {
        self.family.hash_into(digest);
        digest.push_length(self.size);
        digest.push_f32(self.weight);
        self.slant.hash_into(digest);
        digest.push_f32(self.width);
        digest.push(self.synthesis_weight);
        digest.push(self.optical_sizing);
        digest.push(self.variations.len());
        for variation in &self.variations {
            digest.push(variation.tag);
            digest.push_f32(variation.value);
        }
        digest.push(self.features.len());
        for feature in &self.features {
            digest.push(feature.tag);
            digest.push(feature.value);
        }
        self.variant.hash_into(digest);
        match &self.language {
            None => digest.push_tag(0),
            Some(language) => {
                digest.push_tag(1);
                digest.push(language.as_str());
            }
        }
        digest.push(self.language_system);
        self.letter_spacing.hash_into(digest);
        self.word_spacing.hash_into(digest);
        self.line_height.hash_into(digest);
        digest.push(self.word_break);
        digest.push(self.white_space);
        self.transform.hash_into(digest);
    }

    /// Mixes the breaking half into a digest.
    pub fn hash_breaking(&self, digest: &mut Digest) {
        digest.push(self.overflow_wrap);
        digest.push(self.wrap_mode);
        digest.push(self.line_break);
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::initial()
    }
}
