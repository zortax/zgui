//! The order of a display's colour stripes.

/// Which way round a display's subpixels run.
///
/// It decides which channel of a per-channel coverage tile is which, and getting it backwards
/// puts a colour fringe on the wrong side of every stroke.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubpixelOrder {
    /// Red, green, blue — the overwhelmingly common arrangement.
    #[default]
    RedToBlue,
    /// Blue, green, red.
    BlueToRed,
}
