//! The style of a whole inline formatting context, as opposed to one run inside it.

use crate::key::digest::Digest;
use crate::style::spacing::LengthPercent;
use crate::style::writing::WritingMode;

/// The base writing direction a paragraph's bidirectional reordering resolves against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    /// `direction: ltr`.
    LeftToRight,
    /// `direction: rtl`.
    RightToLeft,
}

/// `text-align`, with the two values that depend on the parent already resolved away.
///
/// `match-parent` never reaches here: it is resolved during the cascade, because a run being laid
/// out has no parent style to consult.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextAlign {
    /// `start` — the side lines begin on, which depends on [`Direction`].
    Start,
    /// `end` — the side lines finish on.
    End,
    /// `left`, regardless of direction.
    Left,
    /// `right`, regardless of direction.
    Right,
    /// `center`.
    Center,
    /// `justify` — stretched to both edges, except on the last line.
    Justify,
}

/// `text-align-last` — how the final line of a block is aligned.
///
/// Its own property because the last line is the one [`TextAlign::Justify`] deliberately leaves
/// alone: a justified paragraph whose closing line was also stretched to both edges would end in a
/// row of two-word gaps. `auto` is therefore not "the same as `text-align`" — it means `start`
/// under justification and the paragraph's own alignment otherwise, which is why the keyword
/// survives lowering instead of being folded into [`TextAlign`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextAlignLast {
    /// `auto`.
    Auto,
    /// `start`.
    Start,
    /// `end`.
    End,
    /// `left`.
    Left,
    /// `right`.
    Right,
    /// `center`.
    Center,
    /// `justify`.
    Justify,
}

/// `text-justify` — what a justified line stretches, when it stretches anything.
///
/// A breaking-side property, and one that reaches further than alignment usually does: choosing to
/// distribute space between characters rather than between words changes how much of a line fits
/// before it is stretched at all, so it moves the lines and not only their contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextJustify {
    /// `auto` — whatever suits the script on the line.
    Auto,
    /// `none` — a justified paragraph's lines are not stretched after all.
    None,
    /// `inter-word` — stretch the spaces between words only.
    InterWord,
    /// `inter-character` — stretch between characters as well, which scripts written without
    /// spaces need.
    InterCharacter,
}

/// The indent applied to a paragraph's first line, or to every line but it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextIndent {
    /// How far the indented lines are pushed in. A percentage is of the width the paragraph is
    /// laid out in.
    pub length: LengthPercent,
    /// `hanging` — indent every line *except* the first, instead of the first.
    pub hanging: bool,
    /// `each-line` — indent the line after every forced break, not only the paragraph's first.
    pub each_line: bool,
}

impl TextIndent {
    /// No indent.
    pub const NONE: Self = Self {
        length: LengthPercent::ZERO,
        hanging: false,
        each_line: false,
    };
}

/// The properties an inline formatting context has as a whole.
///
/// Two of these are shaping properties and the rest are breaking ones, which is the same split the
/// run style is built around. [`hash_shaping`](ParagraphStyle::hash_shaping) and
/// [`hash_breaking`](ParagraphStyle::hash_breaking) are where a field is put on one side of the
/// line or the other, and every consumer of the split reads those rather than the fields.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParagraphStyle {
    /// The base direction.
    pub direction: Direction,
    /// The axis the lines run along.
    pub writing_mode: WritingMode,
    /// How lines sit within the available width.
    pub align: TextAlign,
    /// How the final line sits.
    pub align_last: TextAlignLast,
    /// What a justified line stretches.
    pub justify: TextJustify,
    /// The first-line indent.
    pub indent: TextIndent,
}

impl ParagraphStyle {
    /// The style a document with no rules at all resolves to.
    pub fn initial() -> Self {
        Self {
            direction: Direction::LeftToRight,
            writing_mode: WritingMode::HorizontalTb,
            align: TextAlign::Start,
            align_last: TextAlignLast::Auto,
            justify: TextJustify::Auto,
            indent: TextIndent::NONE,
        }
    }

    /// Mixes the paragraph's shaping properties into a digest.
    ///
    /// Neither is obvious. The base direction decides how the bidirectional algorithm resolves the
    /// paragraph, which decides the visual order of the runs and which characters are mirrored. The
    /// writing mode decides whether the face's vertical substitutions apply and which advance table
    /// is read. Both therefore change the glyphs themselves, not merely where the lines fall.
    pub fn hash_shaping(&self, digest: &mut Digest) {
        digest.push(self.direction);
        digest.push(self.writing_mode);
    }

    /// Mixes the paragraph's breaking properties into a digest.
    pub fn hash_breaking(&self, digest: &mut Digest) {
        digest.push(self.align);
        digest.push(self.align_last);
        digest.push(self.justify);
        self.indent.length.hash_into(digest);
        digest.push(self.indent.hanging);
        digest.push(self.indent.each_line);
    }
}

impl Default for ParagraphStyle {
    fn default() -> Self {
        Self::initial()
    }
}
