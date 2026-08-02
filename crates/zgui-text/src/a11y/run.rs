//! The four parallel arrays a text run is described by.

use core::ops::Range;

use accesskit::{Node, TextDirection};
use zgui_geom::CssPx;

/// One selectable unit of shaped text: the bytes it covers and where it sits on its line.
///
/// A cluster rather than a character, because the two are not the same: a base letter and its
/// combining accent are one cluster, and so are the parts of an Indic conjunct, and a caret may not
/// be placed inside either.
#[derive(Clone, Debug, PartialEq)]
pub struct ClusterGeometry {
    /// The byte range of the generated string this cluster covers.
    pub text: Range<usize>,
    /// Distance from the start of the line to the cluster's leading edge.
    pub offset: CssPx,
    /// The cluster's advance.
    pub advance: CssPx,
}

/// One line of text, described the way an accessibility tree wants it.
///
/// The three arrays are parallel and must stay the same length; the byte lengths must sum to the
/// length of the text the run reports. [`is_consistent`](TextRunAttributes::is_consistent) checks
/// both, because a mismatch is read by the platform as a malformed tree rather than reported.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRunAttributes {
    /// How many bytes each cluster occupies.
    pub character_lengths: Vec<u8>,
    /// Where each cluster's leading edge sits, in the line's own coordinates.
    pub character_positions: Vec<f32>,
    /// Each cluster's advance.
    pub character_widths: Vec<f32>,
    /// Which cluster each word starts at.
    pub word_starts: Vec<u8>,
    /// The direction the clusters advance in.
    pub direction: TextDirection,
}

impl TextRunAttributes {
    /// Builds the arrays from one line's clusters.
    ///
    /// `word_starts` is supplied rather than computed: where a word begins is a question about the
    /// text editing model — an editor over source code answers it differently from one over prose —
    /// and answering it here would silently override whatever the caller does when the user presses
    /// a word-wise motion key.
    pub fn from_clusters(
        clusters: &[ClusterGeometry],
        word_starts: Vec<u8>,
        direction: TextDirection,
    ) -> Self {
        Self {
            character_lengths: clusters
                .iter()
                .map(|cluster| u8::try_from(cluster.text.len()).unwrap_or(u8::MAX))
                .collect(),
            character_positions: clusters.iter().map(|cluster| cluster.offset.0).collect(),
            character_widths: clusters.iter().map(|cluster| cluster.advance.0).collect(),
            word_starts,
            direction,
        }
    }

    /// Whether the arrays agree with each other and with `text`.
    pub fn is_consistent(&self, text: &str) -> bool {
        let count = self.character_lengths.len();
        self.character_positions.len() == count
            && self.character_widths.len() == count
            && self
                .word_starts
                .iter()
                .all(|start| usize::from(*start) < count.max(1))
            && self
                .character_lengths
                .iter()
                .map(|length| usize::from(*length))
                .sum::<usize>()
                == text.len()
    }

    /// Writes the arrays onto a node.
    ///
    /// The node's role and value are the caller's, because a text run inside an editable field and
    /// one inside a paragraph carry the same geometry and different roles.
    pub fn apply(&self, node: &mut Node) {
        node.set_character_lengths(self.character_lengths.clone());
        node.set_character_positions(self.character_positions.clone());
        node.set_character_widths(self.character_widths.clone());
        node.set_word_starts(self.word_starts.clone());
        node.set_text_direction(self.direction);
    }
}
