//! Which side of a boundary a caret belongs to.

/// Which of the two places one offset can be drawn at a caret occupies.
///
/// Two situations need this and neither is exotic. A caret at a soft line break can be drawn at the
/// end of the earlier line or at the start of the next one, and the offset is the same number for
/// both. A caret at a direction boundary in bidirectional text has two visual positions for one
/// logical offset — after the last letter of the Arabic run, or before the first letter of the
/// Latin one — and which one is meant is exactly the last direction the caret moved in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Affinity {
    /// The caret belongs to the text *before* the offset: the end of the earlier line, or the
    /// trailing edge of the earlier run.
    #[default]
    Upstream,
    /// The caret belongs to the text *after* the offset: the start of the next line, or the
    /// leading edge of the later run.
    Downstream,
}

impl Affinity {
    /// The other one.
    pub const fn flipped(self) -> Self {
        match self {
            Self::Upstream => Self::Downstream,
            Self::Downstream => Self::Upstream,
        }
    }
}
