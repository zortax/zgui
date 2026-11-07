//! What a frame did, and the one failure that is not a frame at all.

use crate::memory::MemoryReport;

/// What one frame cost.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    /// How many draw calls were issued.
    pub draw_calls: u32,
    /// How many rasterisation passes the vector content cost.
    pub vector_passes: u32,
    /// How many device pixels lay inside the damage rectangles that were redrawn.
    pub damage_px: u64,
    /// How many bytes were copied to the device.
    pub bytes_uploaded: u64,
    /// What was held when the frame finished.
    pub memory: MemoryReport,
}

/// Why a frame did not reach the screen.
///
/// Most variants mean the frame's work **was** submitted: the target it composes into holds this
/// frame's pixels, and its damage is retired. Three of them skip before anything is recorded —
/// [`SkipReason::Unconfigured`], [`SkipReason::Unacquired`] and [`SkipReason::DeviceUnavailable`] —
/// so those three retain damage for the next attempt. [`FrameOutcome::retires_damage`] answers
/// which a variant is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkipReason {
    /// The surface has not been configured yet, so nothing was recorded and nothing was submitted.
    ///
    /// One of the three arms that retains damage. Ask for another frame: configuring the surface is
    /// the first thing the next one does.
    Unconfigured,
    /// Acquiring a surface to present into timed out. Ask for another frame.
    Timeout,
    /// Nothing could be taken to compose into, so nothing was recorded.
    ///
    /// A renderer that composes straight into the buffer a display scans out of is handed that
    /// buffer before it draws, and a display with none free stops the frame there. Ask for another
    /// frame: one refresh interval later a buffer is free. The damage this frame was going to draw
    /// is still owed, so this arm retains it.
    ///
    /// [`SkipReason::Timeout`] is the same refusal met *after* the frame was composed, where the
    /// work was submitted and the damage retires.
    Unacquired,
    /// The window is not visible. **Do not** ask for another frame — waiting for a platform event
    /// or a timer deadline is the difference between a parked loop and a busy one.
    Occluded,
    /// The surface no longer matches the window and has to be reconfigured before the next frame.
    Outdated,
    /// The graphics API rejected something. Logged once and counted; enough of them in a row
    /// escalates to rebuilding the device.
    Validation,
    /// The device was lost and could not be rebuilt, so nothing was recorded.
    ///
    /// The one failure here that another frame cannot improve on. **Do not** ask for another frame:
    /// every other skip describes a moment, and this one describes a machine that has stopped being
    /// able to draw — asking again is a loop rebuilding a device that will not be rebuilt, at
    /// whatever rate the process can manage, logging the same failure each time round.
    DeviceUnavailable,
    /// The frame damaged nothing, so the surface already holds the pixels it would have produced.
    ///
    /// A pointer press over something with no pressed appearance, a key that moved no caret, a
    /// wake for work that turned out to concern another window: each runs the whole pipeline and
    /// arrives here with an empty damage set. Composing and presenting that would copy the target
    /// onto the surface unchanged — and, under a queued presentation mode, spend a swap-chain
    /// image doing it, which is what makes the *next* frame wait a whole refresh interval for one.
    /// **Do not** ask for another frame: nothing changed, so nothing would change next time
    /// either.
    Undamaged,
}

/// What [`Renderer::draw`](crate::Renderer::draw) did.
///
/// Deliberately not a `Result`: most of the ways a frame fails to reach the screen are ordinary
/// events in a window's life, and treating them as errors makes a caller either ignore them or
/// treat a minimised window as a fault.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum FrameOutcome {
    /// The frame reached the screen.
    Presented(FrameStats),
    /// The frame did not reach the screen, for a reason that is usually not a fault.
    Skipped(SkipReason),
    /// The device was lost and has been rebuilt. Everything cached on it is gone and the next frame
    /// redraws the whole surface.
    Recovered,
}

impl FrameOutcome {
    /// Whether the frame's work was submitted, and therefore whether its damage is retired.
    ///
    /// This inverts the naive rule and is the single thing about this type most easily got wrong: a
    /// frame that composed everything and then failed to acquire a surface has still updated the
    /// target it drew into. Redrawing it would repeat work that has already happened.
    pub const fn retires_damage(&self) -> bool {
        !matches!(
            self,
            Self::Skipped(
                SkipReason::Unconfigured | SkipReason::Unacquired | SkipReason::DeviceUnavailable
            )
        )
    }

    /// Whether the caller should ask for another frame.
    ///
    /// False for an occluded surface, which waits for a platform event instead: honouring a redraw
    /// request there is precisely how an invisible window ends up running at full rate.
    ///
    /// True for a frame that was refused what it would compose into. The buffer frees when the flip
    /// holding it completes, and the frame still owes everything it was going to draw.
    pub const fn wants_another_frame(&self) -> bool {
        match self {
            Self::Presented(_) => false,
            Self::Recovered => true,
            Self::Skipped(reason) => !matches!(
                reason,
                SkipReason::Occluded | SkipReason::Undamaged | SkipReason::DeviceUnavailable
            ),
        }
    }

    /// The frame's statistics, if it reached the screen.
    pub const fn stats(&self) -> Option<FrameStats> {
        match self {
            Self::Presented(stats) => Some(*stats),
            _ => None,
        }
    }
}

/// One adapter that was considered and rejected, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RejectedAdapter {
    /// What the adapter called itself.
    pub name: String,
    /// Why it could not be used.
    pub reason: String,
}

/// No usable graphics device could be found.
///
/// This is a typed error and not a fallback, and that is deliberate: a machine with no usable
/// device exists, and silently opening an offscreen surface for a window the user asked for is
/// worse than failing — the window appears and never paints. Every adapter that was tried is listed,
/// because "no device" with no explanation is unactionable.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("no usable graphics device: {} adapter(s) were tried and rejected", candidates.len())]
pub struct GpuUnavailable {
    /// Every adapter that was considered, in the order it was considered.
    pub candidates: Vec<RejectedAdapter>,
}

impl GpuUnavailable {
    /// The failure, with nothing tried yet.
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
        }
    }

    /// Records one rejection.
    pub fn rejected(mut self, name: impl Into<String>, reason: impl Into<String>) -> Self {
        self.candidates.push(RejectedAdapter {
            name: name.into(),
            reason: reason.into(),
        });
        self
    }
}

impl Default for GpuUnavailable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameOutcome, FrameStats, GpuUnavailable, SkipReason};

    /// The rule that is easiest to get backwards, stated as a test rather than as a comment.
    #[test]
    fn damage_is_retired_by_submission_and_not_by_presentation() {
        assert!(FrameOutcome::Presented(FrameStats::default()).retires_damage());
        assert!(FrameOutcome::Skipped(SkipReason::Timeout).retires_damage());
        assert!(FrameOutcome::Skipped(SkipReason::Occluded).retires_damage());
        assert!(FrameOutcome::Skipped(SkipReason::Outdated).retires_damage());
        assert!(FrameOutcome::Skipped(SkipReason::Validation).retires_damage());
        assert!(FrameOutcome::Recovered.retires_damage());

        for arm in [
            SkipReason::Unconfigured,
            SkipReason::Unacquired,
            SkipReason::DeviceUnavailable,
        ] {
            assert!(
                !FrameOutcome::Skipped(arm).retires_damage(),
                "{arm:?} records nothing, so it must keep its damage"
            );
        }
    }

    /// The two skips an acquisition that gave nothing can produce.
    ///
    /// What separates them is when the frame met it. A timeout arrives after the composition, with
    /// the target already holding this frame's pixels; a refused buffer arrives before the frame
    /// drew anything.
    #[test]
    fn a_frame_refused_what_it_would_compose_into_keeps_its_damage() {
        assert!(!FrameOutcome::Skipped(SkipReason::Unacquired).retires_damage());
        assert!(FrameOutcome::Skipped(SkipReason::Timeout).retires_damage());
        assert!(
            FrameOutcome::Skipped(SkipReason::Unacquired).wants_another_frame(),
            "the buffer frees when the flip holding it completes, so the next frame draws what \
             this one still owes"
        );
    }

    #[test]
    fn an_occluded_surface_does_not_ask_for_another_frame() {
        assert!(!FrameOutcome::Skipped(SkipReason::Occluded).wants_another_frame());
        assert!(FrameOutcome::Skipped(SkipReason::Timeout).wants_another_frame());
        assert!(FrameOutcome::Skipped(SkipReason::Unconfigured).wants_another_frame());
        assert!(FrameOutcome::Recovered.wants_another_frame());
        assert!(!FrameOutcome::Presented(FrameStats::default()).wants_another_frame());
    }

    /// The two skips that must never ask again, and why they are not the same reason.
    ///
    /// An occluded surface is waiting for the compositor to show it and a device that cannot be
    /// rebuilt is waiting for nothing at all — but both would spin at whatever rate the process can
    /// manage if the frame that produced them asked for the next one.
    #[test]
    fn a_device_that_cannot_be_rebuilt_does_not_ask_for_another_frame() {
        assert!(!FrameOutcome::Skipped(SkipReason::DeviceUnavailable).wants_another_frame());
        assert!(
            FrameOutcome::Skipped(SkipReason::Validation).wants_another_frame(),
            "one rejected acquisition is a moment, and the run of them is what escalates"
        );
    }

    #[test]
    fn an_unusable_device_names_every_adapter_it_tried() {
        let failure = GpuUnavailable::new()
            .rejected("llvmpipe", "no compute shaders")
            .rejected("integrated", "surface configuration failed");
        assert_eq!(failure.candidates.len(), 2);
        assert!(failure.to_string().contains('2'));
    }
}
