//! Reading a window into the values the panel shows.
//!
//! Everything here runs at the end of a frame with the window in the state that frame left it, and
//! everything it produces is a plain value: the panel never holds a borrow of the document, the
//! layout store or the scene. That is not tidiness — a panel that read the document while drawing
//! would be reading it during the very frame that is rebuilding it, and the borrow would be the
//! second one.

mod element;
pub(crate) mod frame;
pub(crate) mod reactive;
mod timeline;
pub(crate) mod tree;

pub(crate) use crate::sample::element::{Declaration, Element, sample_element};
pub(crate) use crate::sample::frame::{Frame, sample_frame};
pub(crate) use crate::sample::reactive::{Reactive, sample_reactive};
pub(crate) use crate::sample::timeline::{Stage, frame_total_us, sample_timeline};
pub(crate) use crate::sample::tree::{Tree, sample_tree};

/// A number of bytes, spelled the way a person reads it.
pub(crate) fn bytes(count: u64) -> String {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a byte count is shown to one decimal place, so the low bits are not displayed"
    )]
    let scaled = |unit: f64| count as f64 / unit;
    match count {
        0..1024 => format!("{count} B"),
        1024..1_048_576 => format!("{:.1} KiB", scaled(1024.0)),
        1_048_576..1_073_741_824 => format!("{:.1} MiB", scaled(1_048_576.0)),
        _ => format!("{:.2} GiB", scaled(1_073_741_824.0)),
    }
}
