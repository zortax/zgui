//! What a frame is split into before any pass is opened.

use core::ops::Range;

use zgui_geom::{Device, Rect};
use zgui_scene::{Batch, ExternalTextureId};

use crate::frame::target::TargetRef;

/// One draw of a planned pass.
#[derive(Clone, Debug, PartialEq)]
pub enum PlannedDraw {
    /// Clear the scissored region.
    ///
    /// A render pass clears its whole attachment or none of it, so a rectangle that is about to be
    /// redrawn is cleared by drawing over it. This is what makes "redraw only what changed" a real
    /// mechanism rather than a description.
    Clear,
    /// One batch of the display list, drawn instanced out of the frame's buffers.
    Batch(Batch),
    /// One filtering pass of the blur chain, reading `source`.
    Blur {
        /// What it reads.
        source: TargetRef,
        /// The dynamic offset of its block.
        params: u32,
        /// Whether this is the 2:1 downsample rather than one of the two axis passes.
        downsample: bool,
    },
    /// One filtering pass of an application's own shader, reading `source`.
    Effect {
        /// What it reads.
        source: TargetRef,
        /// Which registered effect filters.
        shader: zgui_scene::ShaderId,
        /// The dynamic offset of the block describing what it reads.
        params: u32,
        /// The dynamic offset of the effect's own parameters.
        block: u32,
    },
    /// A composite of an isolated target back into the one beneath it.
    Composite {
        /// What it reads.
        source: TargetRef,
        /// The dynamic offset of its block.
        params: u32,
    },
    /// A rectangle showing a texture the renderer did not draw.
    External {
        /// Which texture.
        texture: ExternalTextureId,
        /// The dynamic offset of its block.
        params: u32,
    },
    /// One rasterised vector pass, composited back into the target.
    ///
    /// It is one draw call whether the pass is composited whole or one item at a time: the two
    /// differ only in how many instances it draws, and which quad and which clip each of them
    /// carries.
    Vector {
        /// Where the rasteriser put the result.
        target: zgui_render::VectorTarget,
        /// The first of this composite's instances.
        first: u32,
        /// How many instances it draws.
        count: u32,
    },
}

/// What happens to a target's existing contents when a pass opens on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassLoad {
    /// Keep them.
    ///
    /// The composed target always keeps them: pixels outside the damage rectangles are last
    /// frame's, which is the whole of what makes redrawing part of a frame legal.
    Keep,
    /// Discard the whole attachment.
    ///
    /// A target lent by the pool is cleared the first time each lease writes into it — the whole
    /// of it, not the region being drawn. That is what makes a blur bleed towards *transparent*
    /// at the edge of what the group painted, which is the edge behaviour CSS specifies, and it is
    /// also what stops a filter reading whatever the previous lease left outside its region.
    Discard,
}

/// A run of draws into one target, under one scissor.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedPass {
    /// Where the draws write.
    pub target: TargetRef,
    /// What happens to what is already there.
    pub load: PassLoad,
    /// The region of that target they may touch.
    pub scissor: Rect<i32, Device>,
    /// The dynamic offset of the block describing this target.
    pub globals: u32,
    /// Which draws, as a range of the plan's draw list.
    pub draws: Range<usize>,
}

/// Something that needs the command encoder itself.
///
/// A live render pass holds the encoder borrowed, so nothing here can happen while one is open.
/// That is the reason the frame is split before any pass is opened rather than during: the split
/// points are exactly these operations, and a planner that names them can be read and tested,
/// while a recorder that discards the borrow to work around them cannot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncoderOp {
    /// Copy a region of one target into another.
    ///
    /// This is how a `backdrop-filter` gets hold of what is beneath it: what it filters is the
    /// composite so far, and a fragment shader cannot read the attachment it is writing.
    Capture {
        /// Where the pixels come from.
        source: TargetRef,
        /// Where they go.
        destination: TargetRef,
        /// The region, in device pixels of the composed target.
        region: Rect<i32, Device>,
    },
}

/// One piece of a planned frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    /// Something needing the encoder, which therefore ends whatever pass preceded it.
    Encoder(EncoderOp),
    /// A run of draws into one target.
    Pass(PlannedPass),
}

impl Segment {
    /// The pass this is, if it is one.
    pub fn pass(&self) -> Option<&PlannedPass> {
        match self {
            Self::Pass(pass) => Some(pass),
            Self::Encoder(_) => None,
        }
    }

    /// The encoder operation this is, if it is one.
    pub fn encoder_op(&self) -> Option<EncoderOp> {
        match self {
            Self::Encoder(op) => Some(*op),
            Self::Pass(_) => None,
        }
    }
}
