//! Where an element's class names live.

use crate::plain_data;

/// A half-open range into the document's class pool.
///
/// Class names are split and interned once, when they are written, and never re-parsed while
/// selectors are being matched. What the record holds is therefore two integers rather than a
/// vector: eight bytes, [`Copy`], and safe to read from a worker thread through a plain cell.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Default, Debug)]
pub struct ClassSpan {
    /// Index of the first name.
    start: u32,
    /// How many names.
    len: u32,
}

impl ClassSpan {
    /// The empty span, which is what an element with no `class` carries.
    pub const EMPTY: Self = Self { start: 0, len: 0 };

    /// The span covering `len` names starting at `start`.
    pub const fn new(start: u32, len: u32) -> Self {
        Self { start, len }
    }

    /// Index of the first name.
    pub const fn start(self) -> u32 {
        self.start
    }

    /// How many names the span covers.
    pub const fn len(self) -> u32 {
        self.len
    }

    /// Whether the span covers no names at all.
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    /// The span as a range into the pool.
    pub const fn range(self) -> core::ops::Range<usize> {
        self.start as usize..(self.start + self.len) as usize
    }
}

plain_data!(ClassSpan);

#[cfg(test)]
mod tests {
    use super::ClassSpan;

    #[test]
    fn a_span_is_eight_bytes_and_describes_its_own_range() {
        assert_eq!(size_of::<ClassSpan>(), 8);
        let span = ClassSpan::new(4, 3);
        assert_eq!(span.range(), 4..7);
        assert_eq!(span.len(), 3);
        assert!(!span.is_empty());
        assert!(ClassSpan::EMPTY.is_empty());
        assert_eq!(ClassSpan::default(), ClassSpan::EMPTY);
    }
}
