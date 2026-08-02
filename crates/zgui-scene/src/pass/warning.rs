//! The diagnostic a frame raises when it plans more passes than the policy expects.

use crate::pass::plan::ScenePassPlan;

/// A frame that planned an unexpected number of passes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassWarning {
    /// How many passes were planned.
    pub passes: usize,
    /// The count at which the warning fires.
    pub threshold: usize,
}

impl PassWarning {
    /// The pass count at which a frame is considered to have mis-planned.
    ///
    /// Each pass is a separate rasteriser submission and is the dominant term in a frame's vector
    /// cost. Three is the most a frame of interleaved vector content is expected to need once
    /// damage has been culled; four is either a genuine full repaint or a coalescing rule that
    /// should not have fired.
    pub const THRESHOLD: usize = 4;

    /// What to say about it.
    pub fn message(&self) -> String {
        format!(
            "planned {} vector passes, at or over the threshold of {}: on a damaged frame this is \
             a coalescing defect",
            self.passes, self.threshold
        )
    }
}

impl ScenePassPlan {
    /// The warning this plan deserves, if any.
    ///
    /// `full_repaint` is whether the frame is redrawing the whole surface. It **suppresses** the
    /// warning rather than rewording it: a full repaint of *n* interleaved regions genuinely needs
    /// *n* passes when something is drawn over each of them, so firing there would mean warning on
    /// every resize of every dashboard — and a warning that fires on every resize is a warning
    /// nobody reads. The pass count is recorded unconditionally either way, so a budget can still
    /// assert the number this declined to shout about.
    pub fn warning(&self, full_repaint: bool) -> Option<PassWarning> {
        (!full_repaint && self.passes.len() >= PassWarning::THRESHOLD).then_some(PassWarning {
            passes: self.passes.len(),
            threshold: PassWarning::THRESHOLD,
        })
    }
}
