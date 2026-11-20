//! Glyph runs captured with a chunk, for a consumer that rasterises glyphs itself.

/// One run of positioned glyphs of one face, as the text emitter drew it.
///
/// The positions are the glyph origins on the baseline, in the coordinates of `transform`, with
/// the pass's displacement applied.
#[derive(Clone, Debug, PartialEq)]
pub struct CapturedRun {
    /// The position in the chunk's operations the run's sprites start at.
    pub at: u32,
    /// The face, as the text engine numbers it.
    pub face: u32,
    /// The em size, in device pixels.
    pub size: f32,
    /// The variation instance, as the text engine numbers it, zero for the default.
    pub instance: u32,
    /// The glyph ids and their origins.
    pub glyphs: Vec<(u16, f32, f32)>,
    /// Premultiplied, gamma-encoded sRGB.
    pub color: [f32; 4],
    /// How much a synthesised bold thickens a stem, in device pixels.
    pub embolden: f32,
    /// The synthesised slant, in degrees.
    pub skew: f32,
    /// Whether the run is a silhouette, such as a text shadow, in which colour glyphs draw nothing.
    pub silhouette: bool,
    /// Whether per-channel coverage was asked for.
    pub subpixel: bool,
    /// The clip chain.
    pub clip: u32,
    /// The slot of the coordinate system.
    pub transform: u32,
}

impl crate::Scene {
    /// Adds `run` to the open chunk capture, at the position the next operation takes.
    ///
    /// Does nothing outside a capture, and pushes nothing into the frame.
    pub fn capture_glyph_run(&mut self, mut run: CapturedRun) {
        if let Some(capture) = &mut self.capture {
            run.at = capture.ops.len() as u32;
            capture.glyph_runs.push(run);
        }
    }
}
