//! How far apart two readings of one frame are, and whether that distance is a difference at all.
//!
//! # Why a comparison of frames has to say this
//!
//! Two engines that lay out the same document by different routes do not produce the same floats.
//! An incremental pass reaches a box's position by adding to what it already had; a pass that was
//! given nothing reaches it by summing from the root; the two sums are the same real number and not
//! the same `f32`. The difference is a few parts in ten million — a ten-thousandth of a device pixel
//! on a page a thousand pixels tall — and no rasteriser in the world can draw it.
//!
//! That is a different thing from a stale pixel, and a gate that reports them with one word reports
//! nothing. So every disagreement is classified before it is counted: what differs, and by how much.
//!
//! * [`Apart::Shape`] — the two lists disagree about *what* is drawn: a primitive, a paint, a clip,
//!   a count. Nothing about float arithmetic can produce this.
//! * [`Apart::Numbers`] — every difference is one number against the same number, and this is the
//!   largest distance any of them moved.
//!
//! A number that moved by less than [`GRID`] is a rounding; anything else is a fault. See [`GRID`]
//! for what that threshold is and what it is not.

/// The finest distinction the frame itself draws, and therefore the coarsest a comparison may
/// ignore.
///
/// A quarter of a percent of a device pixel. Nothing between two positions this close survives into
/// the target: the rasteriser resolves coverage to eight bits along each axis, so two edges a
/// 256th of a pixel apart are the same edge and two glyph origins a 256th apart are the same glyph
/// in the same place.
///
/// It is a floor on what may be dismissed, not a tolerance on what is compared. Everything a
/// display list holds that is *not* a number — every primitive, every paint, every clip, every
/// transform, every position in the painting order — is still compared exactly. The largest
/// rounding this project has ever measured between an incremental frame and a thorough one is four
/// ten-thousandths of a device pixel, a tenth of this, and the mutation test beside the gate moves a
/// rectangle by a 64th to show that four times this is still caught.
pub(crate) const GRID: f32 = 1.0 / 256.0;

/// What separates two readings of one frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Apart {
    /// Every difference is a number against the same number; this is the largest distance moved.
    Numbers(f32),
    /// Something that is not a number differs.
    Shape,
}

impl Apart {
    /// Whether this is a rounding rather than a difference in the picture.
    pub(crate) fn is_rounding(self) -> bool {
        matches!(self, Self::Numbers(most) if most < GRID)
    }

    /// The wider of two, so a step is classified by the worst thing in it.
    pub(crate) fn or_worse(self, other: Self) -> Self {
        match (self, other) {
            (Self::Shape, _) | (_, Self::Shape) => Self::Shape,
            (Self::Numbers(one), Self::Numbers(two)) => Self::Numbers(one.max(two)),
        }
    }
}

impl std::fmt::Display for Apart {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Numbers(most) => write!(formatter, "{most:.6} px"),
            Self::Shape => formatter.write_str("the picture"),
        }
    }
}

/// Nothing moved at all, which is what a step that agrees is separated by.
pub(crate) const TOGETHER: Apart = Apart::Numbers(0.0);

/// How far apart two transcripts are.
pub(crate) fn between(one: &str, two: &str) -> Apart {
    if one.lines().count() != two.lines().count() {
        return Apart::Shape;
    }
    let mut widest = 0.0_f32;
    for (left, right) in one.lines().zip(two.lines()) {
        match line(left, right) {
            Apart::Shape => return Apart::Shape,
            Apart::Numbers(most) => widest = widest.max(most),
        }
    }
    Apart::Numbers(widest)
}

/// How far apart two rectangles are, each held as the bits of its four floats.
pub(crate) fn between_rects(one: &[u32; 4], two: &[u32; 4]) -> Apart {
    let mut widest = 0.0_f32;
    for (left, right) in one.iter().zip(two.iter()) {
        let (left, right) = (f32::from_bits(*left), f32::from_bits(*right));
        if !left.is_finite() || !right.is_finite() {
            if left.to_bits() != right.to_bits() {
                return Apart::Shape;
            }
            continue;
        }
        widest = widest.max((left - right).abs());
    }
    Apart::Numbers(widest)
}

/// How far apart two lines are: [`Apart::Shape`] unless they differ only in their numbers.
fn line(one: &str, two: &str) -> Apart {
    let (mut left, mut right) = (Scan::new(one), Scan::new(two));
    let mut widest = 0.0_f32;
    loop {
        match (left.next(), right.next()) {
            (None, None) => return Apart::Numbers(widest),
            (Some(Piece::Text(one)), Some(Piece::Text(two))) if one == two => {}
            (Some(Piece::Number(one)), Some(Piece::Number(two))) => {
                widest = widest.max((one - two).abs());
            }
            _ => return Apart::Shape,
        }
    }
}

/// One stretch of a line: a number, or everything between two numbers.
enum Piece<'a> {
    /// A run of anything that is not part of a number.
    Text(&'a str),
    /// A number, as it was written.
    Number(f32),
}

/// Walks a line as alternating text and numbers.
struct Scan<'a> {
    /// What is left to walk.
    rest: &'a str,
}

impl<'a> Scan<'a> {
    /// A walk over one line.
    fn new(line: &'a str) -> Self {
        Self { rest: line }
    }
}

impl<'a> Iterator for Scan<'a> {
    type Item = Piece<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        let bytes = self.rest.as_bytes();
        // A number starts at a digit, and takes a leading minus with it. A minus that is not
        // followed by a digit is text, which is what keeps `a-b` one piece of text.
        let starts = |at: usize| {
            bytes[at].is_ascii_digit()
                || (bytes[at] == b'-' && at + 1 < bytes.len() && bytes[at + 1].is_ascii_digit())
        };
        if starts(0) {
            let mut end = usize::from(bytes[0] == b'-');
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            if end < bytes.len() && bytes[end] == b'.' {
                end += 1;
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
            }
            let (number, rest) = self.rest.split_at(end);
            self.rest = rest;
            return Some(Piece::Number(number.parse().unwrap_or(f32::NAN)));
        }
        let mut end = 1;
        while end < bytes.len() && !starts(end) {
            end += 1;
        }
        let (text, rest) = self.rest.split_at(end);
        self.rest = rest;
        Some(Piece::Text(text))
    }
}

#[cfg(test)]
mod tests {
    use super::{Apart, GRID, between, between_rects};

    #[test]
    fn a_number_that_moved_by_a_ten_thousandth_is_a_rounding() {
        let apart = between(
            "quad order=3 bounds=rect(41, 298.9999, 61, 30)",
            "quad order=3 bounds=rect(41, 299, 61, 30)",
        );
        assert!(apart.is_rounding(), "{apart}");
        assert!(
            matches!(apart, Apart::Numbers(most) if most < 0.001),
            "{apart}"
        );
    }

    #[test]
    fn a_number_that_moved_by_a_sixty_fourth_is_not() {
        // The mutation that gives [`GRID`] teeth: four times the threshold, sixteen times the
        // largest rounding ever measured, and a quarter of the smallest thing anyone can see.
        let apart = between(
            "quad order=3 bounds=rect(41, 299, 61, 30)",
            "quad order=3 bounds=rect(41, 299.015625, 61, 30)",
        );
        assert!(!apart.is_rounding(), "{apart}");
    }

    #[test]
    fn a_whole_device_pixel_is_not_a_rounding() {
        let apart = between(
            "group bounds=rect(62, -3940, 123, 54)",
            "group bounds=rect(62, -3939, 123, 54)",
        );
        assert_eq!(apart, Apart::Numbers(1.0));
        assert!(!apart.is_rounding());
    }

    #[test]
    fn a_paint_that_changed_is_never_a_rounding_however_close_the_numbers_are() {
        let apart = between(
            "quad order=3 fill=solid srgb(0, 0.5647, 1, 1)",
            "quad order=3 fill=none srgb(0, 0.5647, 1, 1)",
        );
        assert_eq!(apart, Apart::Shape);
        assert!(!apart.is_rounding());
    }

    #[test]
    fn a_list_that_lost_a_line_differs_in_shape() {
        assert_eq!(between("a 1\nb 2", "a 1"), Apart::Shape);
    }

    #[test]
    fn a_primitive_that_became_another_kind_differs_in_shape() {
        assert_eq!(between("quad order=3", "shadow order=3"), Apart::Shape);
    }

    #[test]
    fn a_negative_number_is_read_as_one_number_and_not_as_a_dash() {
        assert_eq!(between("at -3940", "at -3939"), Apart::Numbers(1.0));
    }

    #[test]
    fn rectangles_are_compared_the_same_way() {
        let apart = between_rects(
            &[1004.0_f32.to_bits(), 0, 0, 0],
            &[1004.0002_f32.to_bits(), 0, 0, 0],
        );
        assert!(apart.is_rounding(), "{apart}");
        assert!(
            !between_rects(
                &[1004.0_f32.to_bits(), 0, 0, 0],
                &[1005.0_f32.to_bits(), 0, 0, 0]
            )
            .is_rounding()
        );
    }

    #[test]
    fn the_threshold_is_finer_than_anything_that_can_be_drawn() {
        // A quarter of a device pixel is a quarter of a device pixel however the constant is
        // written, so this is checked against a number rather than against itself: the eight bits
        // of coverage a rasteriser resolves are the reason the threshold may not grow.
        let widest_a_rasteriser_cannot_see = 1.0 / 256.0;
        assert!(
            GRID <= widest_a_rasteriser_cannot_see,
            "the rasteriser resolves eight bits of coverage",
        );
    }
}
