//! Building and drawing an output's frames.

use compositor_api::Renderer;
use compositor_types::{
    Acquired, BufferAge, Built, FrameClock, FrameOutcome, FrameRequest, PresentMode, SkipReason,
    SubpixelOrder,
};
use web_time::Duration;

use crate::compositor::Client;

/// What drawing an output's frame did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Drawn {
    /// What the renderer or the presenter reported.
    pub(crate) outcome: FrameOutcome,
    /// Whether the compositor built a frame, as opposed to finding nothing changed.
    pub(crate) built: bool,
    /// When the compositor next needs a frame for a scheduled change, if it found nothing changed.
    pub(crate) wake_ns: Option<u64>,
}

impl Client {
    /// Builds the output's next frame and draws it with `renderer`.
    ///
    /// A renderer whose presenter composes into the image it presents has that image acquired
    /// first, and the image's age decides what the frame redraws. Otherwise the age is what the
    /// renderer's kept target holds. `subpixel` is the stripe order per-channel text is drawn for,
    /// `at` the frame's moment, which the compositor samples animations and shader time at, and
    /// `interval` the display's refresh interval.
    ///
    /// The outcome depends on what happened:
    ///
    /// - If no image could be acquired, the frame is skipped for the presenter's reason, and no
    ///   frame is built.
    /// - If nothing changed, the acquired image is given back to the presenter, and the outcome is
    ///   the renderer's `present_held`: a picture that waited for the platform, or one a display
    ///   draws the pointer into, is presented, and otherwise the frame is `Skipped(Undamaged)`.
    /// - Otherwise the frame is drawn, and its outcome is reported to the compositor.
    ///
    /// The counters of a frame that was built are kept for [`Client::take_built`].
    pub(crate) fn draw(
        &mut self,
        renderer: &mut dyn Renderer,
        subpixel: Option<SubpixelOrder>,
        at: zgui_vocab::Timestamp,
        interval: Duration,
    ) -> Drawn {
        let caps = renderer.capabilities();
        let acquired = match acquire(renderer) {
            Ok(acquired) => acquired,
            Err(reason) => {
                return Drawn {
                    outcome: FrameOutcome::Skipped(reason),
                    built: false,
                    wake_ns: None,
                };
            }
        };
        let age = acquired.map_or_else(|| renderer.target_age(), |(_, age)| age);

        let now_ns = nanos(at.since_origin());
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
        let built = self.compositor.compositor.borrow_mut().build_frame(
            self.output,
            &request,
            &mut self.frame,
        );
        if let Built::Idle { wake_ns } = built {
            if let (Some((image, _)), Some(presenter)) = (acquired, renderer.presenter()) {
                presenter.release(image);
            }
            return Drawn {
                outcome: renderer.present_held(),
                built: false,
                wake_ns,
            };
        }
        self.built = Some(self.compositor.compositor.borrow().counters().frame);

        let outcome = renderer.draw(&self.frame);
        self.compositor
            .compositor
            .borrow_mut()
            .frame_done(self.output, self.frame.id, &outcome);
        Drawn {
            outcome,
            built: true,
            wake_ns: None,
        }
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
