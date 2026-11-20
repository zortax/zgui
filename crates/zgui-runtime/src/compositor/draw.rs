//! Building and drawing an output's frames.

use compositor_api::Renderer;
use compositor_types::{
    Acquired, BufferAge, Built, FrameClock, FrameOutcome, FrameRequest, PresentMode, SkipReason,
    SubpixelOrder,
};
use web_time::Duration;

use crate::compositor::Client;

impl Client {
    /// Builds the output's next frame and draws it with `renderer`.
    ///
    /// A renderer whose presenter composes into the image it presents has that image acquired
    /// first, and the image's age decides what the frame redraws. Otherwise the age is what the
    /// renderer's kept target holds. `subpixel` is the stripe order per-channel text is drawn for,
    /// and `interval` the display's refresh interval.
    ///
    /// The outcome depends on what happened:
    ///
    /// - If no image could be acquired, the frame is skipped for the presenter's reason, and no
    ///   frame is built.
    /// - If nothing changed, the frame is `Skipped(Undamaged)`, and the acquired image is given
    ///   back to the presenter.
    /// - Otherwise the frame is drawn, and its outcome is reported to the compositor.
    ///
    /// The counters of a frame that was built are kept for [`Client::take_built`].
    pub(crate) fn draw(
        &mut self,
        renderer: &mut dyn Renderer,
        subpixel: Option<SubpixelOrder>,
        interval: Duration,
    ) -> FrameOutcome {
        let caps = renderer.capabilities();
        let acquired = match acquire(renderer) {
            Ok(acquired) => acquired,
            Err(reason) => return FrameOutcome::Skipped(reason),
        };
        let age = acquired.map_or_else(|| renderer.target_age(), |(_, age)| age);

        let now_ns = nanos(self.started.elapsed());
        let request = FrameRequest {
            caps: &caps,
            age,
            clock: FrameClock {
                now_ns,
                present_ns: now_ns,
                interval_ns: nanos(interval),
            },
            subpixel,
        };
        let built =
            self.compositor
                .borrow_mut()
                .build_frame(self.output, &request, &mut self.frame);
        if let Built::Idle { .. } = built {
            if let (Some((image, _)), Some(presenter)) = (acquired, renderer.presenter()) {
                presenter.release(image);
            }
            return FrameOutcome::Skipped(SkipReason::Undamaged);
        }
        self.built = Some(self.compositor.borrow().counters().frame);

        let outcome = renderer.draw(&self.frame);
        self.compositor
            .borrow_mut()
            .frame_done(self.output, self.frame.id, &outcome);
        outcome
    }
}

/// Acquires the next image of `renderer`'s presenter, when the presenter is in buffer-age mode.
///
/// Returns the image and its age, or `None` for a renderer that composes into a target it keeps.
///
/// # Errors
///
/// Returns the presenter's reason when no image could be acquired.
fn acquire(renderer: &mut dyn Renderer) -> Result<Option<(u32, BufferAge)>, SkipReason> {
    let Some(presenter) = renderer.presenter() else {
        return Ok(None);
    };
    if presenter.capabilities().mode != PresentMode::BufferAge {
        return Ok(None);
    }
    match presenter.acquire() {
        Acquired::Image { image, age } => Ok(Some((image, age))),
        Acquired::Skipped(reason) => Err(reason),
    }
}

/// Returns `duration` in whole nanoseconds, saturating.
fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}
