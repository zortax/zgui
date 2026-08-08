//! This framework's own text properties: the shape a run's style takes once it has left the
//! cascade and before it reaches a shaper.

pub mod face;
pub mod family;
pub mod line_height;
pub mod optical;
pub mod paint;
pub mod paragraph;
pub mod spacing;
pub mod synthesis;
pub mod text;
pub mod transform;
pub mod variant;
pub mod wrap;
pub mod writing;

pub use crate::style::face::{DEFAULT_OBLIQUE_DEGREES, FontFeature, FontSlant, FontVariation, tag};
pub use crate::style::family::{FamilyName, FontFamilyList, GenericFamily};
pub use crate::style::line_height::LineHeight;
pub use crate::style::optical::{OPTICAL_SIZE_AXIS, OpticalSizing};
pub use crate::style::paint::{TextPaint, TextPaintKey};
pub use crate::style::paragraph::{
    Direction, ParagraphStyle, TextAlign, TextAlignLast, TextIndent, TextJustify,
};
pub use crate::style::spacing::LengthPercent;
pub use crate::style::synthesis::SynthesisWeight;
pub use crate::style::text::TextStyle;
pub use crate::style::transform::{CaseTransform, TextTransform};
pub use crate::style::variant::FontVariant;
pub use crate::style::wrap::{LineBreak, OverflowWrap, WhiteSpaceCollapse, WordBreak, WrapMode};
pub use crate::style::writing::WritingMode;
