//! What an inline formatting context is made of, and the one string a shaper is handed.
//!
//! A shaper takes a flat string with a list of styled ranges. An inline formatting context is a
//! tree: text, images and `inline-block`s interleaved, nested inside spans that carry their own
//! fonts, margins, borders and padding. Flattening the second into the first is what happens here,
//! and it is not a lossless copy — white space collapses, tabs expand, and an inline box's edges
//! occupy width that belongs to no character at all.
//!
//! Two things are therefore produced beside the string. The offset map is how every hit test,
//! caret and selection gets back to the text the document actually holds, and the item list is how
//! every atomic inline and every inline-box edge is found again in the shaper's answer.

pub(crate) mod collect;
pub(crate) mod generate;
pub(crate) mod memo;
pub(crate) mod styles;

use std::sync::{Arc, OnceLock};

use zgui_dom::side::BoxKey;
use zgui_text::{StyledRun, TextMap};
use zgui_text_style::{ParagraphStyle, TextStyle};

/// What one of the shaper's inline boxes stands for.
///
/// A shaper knows only that something opaque of a given size sits between two characters. Which of
/// the two things that can be — a box with content of its own, or the edge of a span that has
/// margins — is known only here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    /// An atomic inline: its own margin box sits on the line and its contents are laid out
    /// separately.
    Atomic(BoxKey),
    /// The start edge of a nested inline box — its left margin, border and padding.
    StartEdge(BoxKey),
    /// Its end edge.
    EndEdge(BoxKey),
}

impl Role {
    /// The box this stands for.
    pub(crate) fn box_(self) -> BoxKey {
        match self {
            Self::Atomic(key) | Self::StartEdge(key) | Self::EndEdge(key) => key,
        }
    }

    /// Whether this is a box whose own contents have to be laid out.
    pub(crate) fn is_atomic(self) -> bool {
        matches!(self, Self::Atomic(_))
    }
}

/// One thing the shaper packs between the words.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Item {
    /// How the shaper names it, stable for as long as the content is.
    pub(crate) id: u64,
    /// What it stands for.
    pub(crate) role: Role,
    /// The byte offset in the generated string it sits at.
    pub(crate) offset: usize,
}

/// One inline formatting context, flattened into what a shaper takes.
///
/// Everything here is a function of the boxes and their styles alone, so it survives every width
/// the surrounding algorithm probes with and is rebuilt only when the content or the styles change.
/// What is *not* here is any measurement: an atomic inline's size and its alignment shift are
/// resolved afresh on every measure call, because both can move while this stays valid.
#[derive(Clone, Debug)]
pub(crate) struct Generated {
    /// The part of the shaping key no measurement can move, computed at most once however many
    /// widths layout probes.
    key: OnceLock<zgui_text::ContentKey>,
    /// The string the shaper is handed.
    pub(crate) text: String,
    /// How to get from an offset in it back to the document.
    pub(crate) map: TextMap,
    /// The styled ranges, covering the string with no gaps.
    pub(crate) runs: Vec<StyledRun>,
    /// The box each text run's characters came from, parallel to the run indices the offset map
    /// reports.
    pub(crate) sources: Vec<BoxKey>,
    /// The atomic inlines and inline-box edges, in ascending offset order.
    pub(crate) items: Vec<Item>,
    /// What the context has as a whole.
    pub(crate) paragraph: ParagraphStyle,
    /// The style the strut is measured from, which is the establishing block's own.
    pub(crate) root: Arc<TextStyle>,
    /// The brush the establishing block's own text is drawn with.
    ///
    /// Claimed for the block rather than for any run in it, and used by anything the *box* draws
    /// rather than the text inside it — which today is the mark `text-overflow` writes where a line
    /// was cut. A run inside the block may be another colour, and the mark saying the box cut its
    /// content is not part of that run.
    pub(crate) root_brush: zgui_text::Brush,
}

impl Generated {
    /// The cache key for this flattened context, at the widths its boxes measure now.
    ///
    /// Only the part the boxes cannot move is held: an identifier is a position in *this* flattened
    /// form and starts again at zero in the next, so it says nothing about how wide the box behind
    /// it came out. The widths are folded in per call, and a context whose atomic inline is a
    /// different size is a different entry.
    pub(crate) fn key(&self, content: &zgui_text::ParagraphContent<'_>) -> zgui_text::ParagraphKey {
        self.key
            .get_or_init(|| zgui_text::ContentKey::of(content))
            .with_boxes(content.boxes)
    }

    /// The item one of the shaper's inline boxes belongs to.
    pub(crate) fn item(&self, id: u64) -> Option<&Item> {
        self.items.iter().find(|item| item.id == id)
    }
}
