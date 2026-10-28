//! What one buffer is, and what part of it a copy names.

use std::os::fd::BorrowedFd;

/// A rectangle of pixels, in the buffer's own coordinates.
///
/// Plain numbers rather than a geometry type: this crate names no other crate in the tree, so a
/// caller converts at the boundary. The origin is the top-left corner, which is where a scanout
/// buffer's first pixel is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Distance from the left edge, in pixels.
    pub x: i32,
    /// Distance from the top edge, in pixels.
    pub y: i32,
    /// How wide, in pixels.
    pub width: i32,
    /// How tall, in pixels.
    pub height: i32,
}

impl Rect {
    /// Returns `true` where this covers no pixel at all.
    ///
    /// A copy of one is not an error and does nothing, so callers may pass a damage set through
    /// without sifting it first.
    pub const fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }
}

/// One buffer, named by the descriptor it was exported as.
///
/// Everything here is what the exporter said about the memory, and all of it has to be right: a
/// stride or a layout that disagrees with the allocation is read as pixels anyway, and reaches the
/// screen as a picture that is skewed or striped rather than as a refusal.
#[derive(Debug, Clone, Copy)]
pub struct Buffer<'a> {
    /// The dma-buf the memory was exported as.
    pub descriptor: BorrowedFd<'a>,
    /// How wide it is, in pixels.
    pub width: u32,
    /// How tall it is, in pixels.
    pub height: u32,
    /// The fourcc the pixels are arranged in, the same code a framebuffer is registered under.
    pub fourcc: u32,
    /// How long a row is, in bytes.
    pub stride: u32,
    /// Where the pixels start.
    pub offset: u32,
    /// The layout the allocator chose, where it named one.
    ///
    /// [`None`] for an implicit layout, and that is a different statement from naming
    /// `DRM_FORMAT_MOD_INVALID`. A chain of buffers is wholly implicit or wholly explicit, so a
    /// caller that allocated without naming a layout passes [`None`] here as well.
    pub modifier: Option<u64>,
}
