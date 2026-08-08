//! Holding one flattened inline formatting context between layout passes.
//!
//! Flattening walks every character of a paragraph to build the shaper's string, its offset map and
//! its styled runs, and the result depends on the boxes, their styles and the device scale — never
//! on the width the paragraph is being asked about. A layout algorithm asks a paragraph how big it
//! is at a dozen widths, and a document is laid out again whenever anything at all moves, so a
//! flattened form that lived only as long as one pass would be rebuilt character by character for
//! every one of those questions.
//!
//! What is held is therefore kept beside the box, and what makes holding it safe is that it carries
//! the two things it was built from and is checked against them rather than being invalidated from
//! a distance. A caller that changed the content cannot forget to say so.

use std::sync::Arc;

use crate::inline::content::Generated;
use crate::inline::content::collect::Piece;

/// One inline formatting context's flattened form, and what it was flattened from.
#[derive(Debug)]
pub(crate) struct Flattened {
    /// The device scale it was built at, which its paragraph indent is already expressed in.
    scale: f32,
    /// The pieces it was built from, in document order.
    pieces: Vec<Piece>,
    /// The flattened form itself.
    content: Arc<Generated>,
}

impl Flattened {
    /// Holds `content`, recording the scale and the pieces it was built from.
    pub(crate) fn new(scale: f32, pieces: Vec<Piece>, content: Arc<Generated>) -> Self {
        Self {
            scale,
            pieces,
            content,
        }
    }

    /// The held form, if it was built from exactly these pieces at this scale.
    ///
    /// A piece names a box, and a box's text and style are fixed when it is built — a restyle or an
    /// edit replaces the box rather than rewriting it — so an unchanged sequence of pieces is
    /// unchanged content. A box added to or removed from the context changes the sequence, and a
    /// scale change changes what the lengths in the paragraph style mean.
    pub(crate) fn reuse(&self, scale: f32, pieces: &[Piece]) -> Option<Arc<Generated>> {
        (self.scale == scale && self.pieces == pieces).then(|| Arc::clone(&self.content))
    }

    /// The string a shaper was handed.
    pub(crate) fn text(&self) -> &str {
        &self.content.text
    }
}
