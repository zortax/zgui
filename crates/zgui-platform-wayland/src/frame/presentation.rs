//! Asking the compositor when a frame actually reached the screen.

use std::time::Duration;

use smithay_client_toolkit::globals::GlobalData;
use smithay_client_toolkit::reexports::client::globals::GlobalList;
use smithay_client_toolkit::reexports::client::{Connection, Dispatch, Proxy, QueueHandle};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_protocols::wp::presentation_time::client::wp_presentation::{self, WpPresentation};
use wayland_protocols::wp::presentation_time::client::wp_presentation_feedback::{
    self, WpPresentationFeedback,
};
use zgui_platform::SurfaceId;

use crate::driver::WaylandState;

/// The clock identifier every compositor worth trusting reports.
///
/// The protocol lets a compositor name any of the system's clocks. Only the monotonic one can be
/// placed on this process's own timeline, so anything else is refused rather than mixed in: a phase
/// derived from the wall clock jumps by hours whenever the machine syncs its time.
const CLOCK_MONOTONIC: u32 = 1;

/// The presentation-time global, when the compositor offers one.
///
/// It is the source of the two numbers a frame schedule is built on — the moment a frame reached
/// the screen and the interval of the output it reached — and both are per surface and restated
/// per frame, which is what makes a window dragged between two monitors follow the second one.
#[derive(Debug, Default, Clone)]
pub struct Presentation {
    /// The global itself.
    global: Option<WpPresentation>,
    /// Whether the compositor's clock is one this process can read.
    usable: bool,
}

impl Presentation {
    /// Binds the global, when the compositor advertised it.
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<WaylandState>) -> Self {
        Self {
            global: globals.bind(qh, 1..=1, GlobalData).ok(),
            // Assumed until the compositor says otherwise, which it does immediately after binding
            // and before any feedback can arrive.
            usable: true,
        }
    }

    /// Records which clock the compositor stamps its answers in.
    pub fn clock(&mut self, id: u32) {
        self.usable = id == CLOCK_MONOTONIC;
        if !self.usable {
            tracing::warn!(
                clock = id,
                "the compositor times presentation against a clock this process cannot read; \
                 frames will be paced against the output's declared rate instead"
            );
        }
    }

    /// Asks for feedback on the content update `surface` is about to commit.
    ///
    /// Asked per update rather than once, which is what the protocol requires: each answer belongs
    /// to exactly one commit, and the object is retired by answering.
    pub fn ask(&self, qh: &QueueHandle<WaylandState>, surface: &WlSurface, id: SurfaceId) {
        if !self.usable {
            return;
        }
        if let Some(global) = &self.global {
            global.feedback(surface, qh, id);
        }
    }
}

/// The refresh interval a feedback event reports.
///
/// Zero means the output has no fixed rate, and is passed through as zero: the timing that records
/// it is where "no fixed rate" is turned into "unknown", so that both callers agree.
pub const fn refresh(nanoseconds: u32) -> Duration {
    Duration::from_nanos(nanoseconds as u64)
}

impl Dispatch<WpPresentation, GlobalData> for WaylandState {
    fn event(
        state: &mut Self,
        _presentation: &WpPresentation,
        event: <WpPresentation as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wp_presentation::Event::ClockId { clk_id } = event {
            state.presentation_clock(clk_id);
        }
    }
}

impl Dispatch<WpPresentationFeedback, SurfaceId> for WaylandState {
    fn event(
        state: &mut Self,
        _feedback: &WpPresentationFeedback,
        event: <WpPresentationFeedback as Proxy>::Event,
        id: &SurfaceId,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wp_presentation_feedback::Event::Presented {
                tv_sec_hi,
                tv_sec_lo,
                tv_nsec,
                refresh: nanoseconds,
                ..
            } => {
                let seconds = (u64::from(tv_sec_hi) << 32) | u64::from(tv_sec_lo);
                state.frame_presented(*id, seconds, tv_nsec, refresh(nanoseconds));
            }
            wp_presentation_feedback::Event::Discarded => state.frame_discarded(*id),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Presentation, refresh};
    use std::time::Duration;

    #[test]
    fn a_compositor_timing_against_the_wall_clock_is_not_believed() {
        let mut presentation = Presentation::default();
        presentation.clock(1);
        assert!(presentation.usable);
        presentation.clock(0);
        assert!(
            !presentation.usable,
            "the wall clock jumps; the phase must not"
        );
    }

    #[test]
    fn an_output_with_no_fixed_rate_reports_an_interval_of_nothing() {
        assert_eq!(refresh(0), Duration::ZERO);
        assert_eq!(refresh(13_346_680), Duration::from_nanos(13_346_680));
    }
}
