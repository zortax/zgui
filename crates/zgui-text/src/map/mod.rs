//! The generated-text ↔ source-text offset map.
//!
//! A shaper is handed one flat string the document never contained. White space has been collapsed,
//! tabs may have been expanded, `text-transform` may have changed the letters, and a control
//! forcing the paragraph's base direction has been prefixed. Every offset a shaper reports —
//! cluster ranges, cursor positions, hit-test results — is an offset into *that* string, so every
//! selection, caret, accessibility range and hit test has to be mapped back.
//!
//! ```
//! use zgui_text::{SourcePos, TextMap};
//!
//! // "  hello" in the source became "hello" in the generated string.
//! let mut map = TextMap::new();
//! map.push(0..5, 0, 2);
//!
//! assert_eq!(map.to_source(0), Some(SourcePos { run: 0, offset: 2 }));
//! assert_eq!(map.to_generated(SourcePos { run: 0, offset: 4 }), Some(2));
//! ```

pub mod segment;

pub use crate::map::segment::{Segment, SourcePos};

use core::ops::Range;

use smallvec::SmallVec;

/// Maps generated offsets to source positions and back.
///
/// Built by whatever generates the string, one verbatim stretch at a time, and consulted
/// afterwards. Stretches are recorded in ascending generated order and merged when they are
/// contiguous in both strings, so text that survived collapsing untouched costs one entry however
/// long it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextMap {
    /// Ordered, non-overlapping, ascending in the generated start offset.
    segments: SmallVec<[Segment; 4]>,
}

impl TextMap {
    /// An empty map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that `generated` is a verbatim copy of the bytes at `offset` in `run`.
    ///
    /// Empty stretches are ignored, and a stretch contiguous with the previous one in both strings
    /// extends it rather than adding an entry.
    pub fn push(&mut self, generated: Range<usize>, run: usize, offset: usize) {
        if generated.is_empty() {
            return;
        }
        if let Some(last) = self.segments.last_mut()
            && last.run == run
            && last.generated.end == generated.start
            && last.offset + last.generated.len() == offset
        {
            last.generated.end = generated.end;
            return;
        }
        self.segments.push(Segment {
            generated,
            run,
            offset,
        });
    }

    /// Every recorded stretch.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// Whether nothing has been recorded.
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// The source position a generated offset came from.
    ///
    /// Nothing is returned for an offset inside text the source never held: the direction control at
    /// the front, and every byte past the first of a white-space run that collapsed to one space.
    /// Those offsets are still hit-testable clusters, so a caller that has one snaps to the nearest
    /// real source position rather than propagating the absence.
    pub fn to_source(&self, generated: usize) -> Option<SourcePos> {
        let index = self
            .segments
            .partition_point(|segment| segment.generated.end <= generated);
        let segment = self.segments.get(index)?;
        if !segment.generated.contains(&generated) {
            return None;
        }
        Some(SourcePos {
            run: segment.run,
            offset: segment.offset + (generated - segment.generated.start),
        })
    }

    /// The nearest source position at or after a generated offset, or the end of the text when
    /// there is nothing after it.
    ///
    /// This is what a hit test on a control or a collapsed space wants: those positions have no
    /// source of their own, but a caret placed there has to land somewhere real. An offset past
    /// every recorded stretch — which a control at the very end of the generated string is — snaps
    /// backwards to the end of the last one instead, because "nowhere after this" and "nowhere at
    /// all" are different answers and only the second may be reported as absent.
    ///
    /// Nothing at all is returned only when the map is empty, which means no generated offset has
    /// a source and there is nothing to snap to.
    ///
    /// ```
    /// use zgui_text::{SourcePos, TextMap};
    ///
    /// // "ab" followed by a control the source never held.
    /// let mut map = TextMap::new();
    /// map.push(0..2, 0, 0);
    ///
    /// assert_eq!(map.to_source(2), None);
    /// assert_eq!(map.to_source_snapped(2), Some(SourcePos { run: 0, offset: 2 }));
    /// assert_eq!(TextMap::new().to_source_snapped(0), None);
    /// ```
    pub fn to_source_snapped(&self, generated: usize) -> Option<SourcePos> {
        let index = self
            .segments
            .partition_point(|segment| segment.generated.end <= generated);
        let segment = self.segments.get(index).or_else(|| self.segments.last())?;
        let within = generated.saturating_sub(segment.generated.start);
        Some(SourcePos {
            run: segment.run,
            offset: segment.offset + within.min(segment.generated.len()),
        })
    }

    /// The generated offset a source position maps to, allowing the position just past a stretch.
    ///
    /// This is what a caret wants and [`to_generated`](TextMap::to_generated) is not: the caret at
    /// the end of the text sits at an offset equal to the text's length, and no byte of the source
    /// is at that offset, so the plain lookup answers nothing and the caret at the end of every
    /// field is simply absent. The end of a stretch is a real place — the trailing edge of its last
    /// cluster — and it is reported here.
    ///
    /// ```
    /// use zgui_text::{SourcePos, TextMap};
    ///
    /// let mut map = TextMap::new();
    /// map.push(0..2, 0, 0);
    ///
    /// assert_eq!(map.to_generated(SourcePos { run: 0, offset: 2 }), None);
    /// assert_eq!(map.to_generated_snapped(SourcePos { run: 0, offset: 2 }), Some(2));
    /// assert_eq!(map.to_generated_snapped(SourcePos { run: 1, offset: 0 }), None);
    /// ```
    pub fn to_generated_snapped(&self, position: SourcePos) -> Option<usize> {
        if let Some(generated) = self.to_generated(position) {
            return Some(generated);
        }
        self.segments.iter().find_map(|segment| {
            (segment.run == position.run
                && position.offset == segment.offset + segment.generated.len())
            .then_some(segment.generated.end)
        })
    }

    /// The generated offset a source position maps to, if it survived generation.
    pub fn to_generated(&self, position: SourcePos) -> Option<usize> {
        self.segments.iter().find_map(|segment| {
            (segment.run == position.run
                && position.offset >= segment.offset
                && position.offset < segment.offset + segment.generated.len())
            .then(|| segment.generated.start + (position.offset - segment.offset))
        })
    }
}
