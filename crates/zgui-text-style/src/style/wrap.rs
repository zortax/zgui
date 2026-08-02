//! Where breaks are allowed, and what happens to white space before shaping.

/// `word-break` — where a break is permitted at all.
///
/// This is a shaping-side property rather than a breaking-side one: it changes which positions are
/// break opportunities, and a shaper records those alongside the clusters it produces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WordBreak {
    /// `normal` — break at the usual word boundaries for the text's script.
    Normal,
    /// `break-all` — break between any two characters, for scripts written without spaces.
    BreakAll,
    /// `keep-all` — never break within a run of CJK characters.
    KeepAll,
}

/// `overflow-wrap` — whether a word too long for its line may be broken to avoid overflowing.
///
/// A breaking-side property: it changes which of the already recorded opportunities may be taken
/// when nothing else fits, and never which opportunities exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverflowWrap {
    /// `normal` — an over-long word overflows.
    Normal,
    /// `break-word` — break an over-long word, but only if it would otherwise overflow.
    BreakWord,
    /// `anywhere` — as `break-word`, and the break also counts towards the minimum content size.
    Anywhere,
}

/// `line-break` — how strictly the rules for breaking beside CJK punctuation are applied.
///
/// A breaking-side property. It changes whether a line may end before a small kana or a closing
/// bracket, which moves where the lines fall and never which glyphs are drawn — the same shaped
/// text is simply cut in different places.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineBreak {
    /// `auto` — the strictness the platform judges right for the content.
    Auto,
    /// `loose` — the fewest restrictions, as newspapers set narrow columns.
    Loose,
    /// `normal` — the common restrictions.
    Normal,
    /// `strict` — the most restrictions.
    Strict,
    /// `anywhere` — a break is allowed between any two characters, ignoring every prohibition.
    Anywhere,
}

/// `text-wrap-mode` — whether soft wrapping happens at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WrapMode {
    /// `wrap` — lines are broken to fit.
    Wrap,
    /// `nowrap` — only forced breaks end a line.
    NoWrap,
}

/// `white-space-collapse` — what happens to spaces, tabs and newlines before anything is shaped.
///
/// A shaping-side property, and the strongest one there is: it decides what text the shaper is
/// handed in the first place, so changing it changes every cluster.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WhiteSpaceCollapse {
    /// `collapse` — runs of white space become one space and newlines are white space.
    Collapse,
    /// `preserve` — every space, tab and newline survives, and newlines force a break.
    Preserve,
    /// `preserve-breaks` — white space collapses, but newlines still force a break.
    PreserveBreaks,
    /// `break-spaces` — as `preserve`, and a line may break after any preserved space.
    BreakSpaces,
}

impl WhiteSpaceCollapse {
    /// Whether runs of white space are replaced by a single space.
    pub fn collapses_spaces(self) -> bool {
        matches!(self, Self::Collapse | Self::PreserveBreaks)
    }

    /// Whether a newline in the source forces a line break.
    pub fn preserves_newlines(self) -> bool {
        matches!(
            self,
            Self::Preserve | Self::PreserveBreaks | Self::BreakSpaces
        )
    }
}

#[cfg(test)]
mod tests {
    use super::WhiteSpaceCollapse;

    #[test]
    fn the_two_halves_of_collapsing_are_answered_separately() {
        // The four values exist because the two questions are independent, and each of the four
        // answers a different pair — a predicate that folded them together would agree with this
        // on at most three of the eight answers below.
        let rows = [
            (WhiteSpaceCollapse::Collapse, true, false),
            (WhiteSpaceCollapse::Preserve, false, true),
            (WhiteSpaceCollapse::PreserveBreaks, true, true),
            (WhiteSpaceCollapse::BreakSpaces, false, true),
        ];
        for (value, collapses, newlines) in rows {
            assert_eq!(
                value.collapses_spaces(),
                collapses,
                "{value:?} collapses runs of white space"
            );
            assert_eq!(
                value.preserves_newlines(),
                newlines,
                "{value:?} lets a source newline force a break"
            );
        }
    }
}
