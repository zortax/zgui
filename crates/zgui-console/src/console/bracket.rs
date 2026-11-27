//! The two calls that bracket a frame drawn straight into a display's own buffer.

use iceblink::{FrameOutcome, SkipReason};
use tracing::warn;
use zgui_geom::{Device, Rect};
use zgui_platform::PlatformError;
use zgui_platform_drm::DrmDisplay;

/// The two calls that bracket a frame drawn straight into a display's own buffer.
///
/// [`DrmDisplay`] is the one implementation. The trait lets the order be asserted with no graphics
/// device and no display open, and that order is the one property on this path that fails
/// silently when it is wrong.
pub(crate) trait Bracket {
    /// Takes the buffer the next frame goes into back from the display engine, and names it.
    fn acquire(&self) -> Result<Option<usize>, PlatformError>;

    /// Gives the buffer the frame was copied into to the display engine, and shows it.
    ///
    /// `damage` is what the frame changed, compared with the frame given over before it.
    fn present_drawn(&self, damage: &[Rect<i32, Device>]) -> Result<bool, PlatformError>;
}

impl Bracket for DrmDisplay {
    fn acquire(&self) -> Result<Option<usize>, PlatformError> {
        DrmDisplay::acquire(self)
    }

    fn present_drawn(&self, damage: &[Rect<i32, Device>]) -> Result<bool, PlatformError> {
        DrmDisplay::present_drawn(self, damage)
    }
}

/// Runs one frame on the drawn path: take a buffer back, compose, give the buffer over.
///
/// `compose` is handed the buffer the frame is copied into, or `None` when the display has no
/// buffer free, and answers what the frame did with the rectangles it changed. It runs between the
/// two calls, and it runs on every frame, because a renderer applies a frame's tables whether or
/// not it presents it.
///
/// The outcome depends on what `compose` did:
///
/// - If it held the frame back for want of a buffer, the frame is `Skipped(Declined)`: the
///   renderer's target holds it, its damage retires, and another frame is asked for, which gives
///   it over once a buffer is free.
/// - If it presented the frame into the buffer, the buffer is given over, and a display that
///   refuses it makes the frame `Skipped(Timeout)` or `Skipped(Validation)`.
/// - Otherwise its outcome stands and nothing is given over. A buffer taken back stays on this
///   side of the handover, so the next acquire names it again.
pub(crate) fn drawn_into_scanout(
    display: &dyn Bracket,
    compose: impl FnOnce(Option<usize>) -> (FrameOutcome, Vec<Rect<i32, Device>>),
) -> FrameOutcome {
    let slot = match display.acquire() {
        Ok(slot) => slot,
        Err(error) => {
            warn!("the buffer a frame would be drawn into could not be taken back: {error}");
            None
        }
    };

    let (drawn, damage) = compose(slot);
    if drawn == FrameOutcome::Skipped(SkipReason::Held) {
        return FrameOutcome::Skipped(SkipReason::Declined);
    }
    if slot.is_none() || !matches!(drawn, FrameOutcome::Presented(_)) {
        return drawn;
    }

    match display.present_drawn(&damage) {
        // The frame reached the display engine. It goes on the screen at the next vertical blank,
        // either as the flip this asked for or as the frame the outstanding flip's completion
        // commits.
        Ok(true) => drawn,
        // The acquire above answered, so the display had a buffer for this frame. Answering that it
        // has none now is a display that lost track of it in between.
        Ok(false) => FrameOutcome::Skipped(SkipReason::Timeout),
        Err(error) => {
            warn!("a frame could not be given to its display: {error}");
            FrameOutcome::Skipped(SkipReason::Validation)
        }
    }
}

#[cfg(test)]
mod tests {
    //! The order a drawn frame keeps, and what a frame that was not presented leaves behind.
    //!
    //! `tests/imported.rs` in `zgui-platform-drm` runs the same order against a real driver and a
    //! real kernel.

    use std::cell::{Cell, RefCell};

    use iceblink::FrameStats;

    use super::*;

    /// How many buffers the imported shape drives a display from.
    const BUFFERS: usize = 3;

    /// A frame that reached the buffer it was pointed at.
    fn presented() -> FrameOutcome {
        FrameOutcome::Presented(FrameStats::default())
    }

    /// A display that answers the way an imported scanout does, writing down what it was asked.
    ///
    /// `present_drawn` refuses a buffer the display engine already has and is the only thing that
    /// moves to the next buffer, so a frame that gives nothing over leaves both where they were.
    struct Rotating {
        log: RefCell<Vec<String>>,
        back: Cell<usize>,
        foreign: RefCell<[bool; BUFFERS]>,
        busy: Cell<bool>,
    }

    impl Rotating {
        fn new() -> Self {
            Self {
                log: RefCell::new(Vec::new()),
                back: Cell::new(0),
                foreign: RefCell::new([false; BUFFERS]),
                busy: Cell::new(false),
            }
        }

        /// A display with every buffer either on the screen or named by a flip on its way.
        fn flipping() -> Self {
            let display = Self::new();
            display.busy.set(true);
            display
        }

        fn note(&self, step: impl Into<String>) {
            self.log.borrow_mut().push(step.into());
        }

        fn steps(&self) -> Vec<String> {
            self.log.borrow().clone()
        }

        /// One frame drawn through this display. A frame with no buffer is held back, as a
        /// renderer whose present gate is closed holds it.
        fn frame(&self, composed: FrameOutcome) -> FrameOutcome {
            drawn_into_scanout(self, |slot| match slot {
                Some(slot) => {
                    self.note(format!("compose into {slot}"));
                    (composed, Vec::new())
                }
                None => {
                    self.note("compose into the renderer's target");
                    (FrameOutcome::Skipped(SkipReason::Held), Vec::new())
                }
            })
        }
    }

    impl Bracket for Rotating {
        fn acquire(&self) -> Result<Option<usize>, PlatformError> {
            if self.busy.get() {
                self.note("acquire: no buffer is free");
                return Ok(None);
            }
            let back = self.back.get();
            self.foreign.borrow_mut()[back] = false;
            self.note(format!("acquire {back}"));
            Ok(Some(back))
        }

        fn present_drawn(&self, _damage: &[Rect<i32, Device>]) -> Result<bool, PlatformError> {
            let back = self.back.get();
            if self.foreign.borrow()[back] {
                return Err(PlatformError::Backend(format!(
                    "the display engine already holds buffer {back}"
                )));
            }
            self.foreign.borrow_mut()[back] = true;
            self.back.set((back + 1) % BUFFERS);
            self.note(format!("give over {back}"));
            Ok(true)
        }
    }

    /// A display whose graphics device will not hand a buffer back at all.
    struct Refusing;

    impl Bracket for Refusing {
        fn acquire(&self) -> Result<Option<usize>, PlatformError> {
            Err(PlatformError::Backend(
                "the device did not finish the barrier".to_owned(),
            ))
        }

        fn present_drawn(&self, _damage: &[Rect<i32, Device>]) -> Result<bool, PlatformError> {
            panic!("a frame no buffer was taken back for was given to the display engine");
        }
    }

    #[test]
    fn a_frame_is_composed_between_the_two_calls_that_bracket_it() {
        let display = Rotating::new();

        assert_eq!(display.frame(presented()), presented());

        assert_eq!(
            display.steps(),
            ["acquire 0", "compose into 0", "give over 0"],
            "the buffer is taken back, drawn into, and only then given over"
        );
    }

    #[test]
    fn each_frame_goes_into_the_buffer_the_display_named_for_it() {
        let display = Rotating::new();

        for _ in 0..BUFFERS + 1 {
            assert_eq!(display.frame(presented()), presented());
        }

        let expected: Vec<String> = [0, 1, 2, 0]
            .iter()
            .flat_map(|slot| {
                [
                    format!("acquire {slot}"),
                    format!("compose into {slot}"),
                    format!("give over {slot}"),
                ]
            })
            .collect();
        assert_eq!(
            display.steps(),
            expected,
            "the frame follows the display round its buffers rather than choosing one itself"
        );
    }

    #[test]
    fn a_frame_with_no_free_buffer_is_held_in_the_renderer_and_given_over_nowhere() {
        let display = Rotating::flipping();

        assert_eq!(
            display.frame(presented()),
            FrameOutcome::Skipped(SkipReason::Declined)
        );
        assert_eq!(
            display.steps(),
            [
                "acquire: no buffer is free",
                "compose into the renderer's target"
            ],
            "the frame was composed and nothing was given over"
        );
    }

    #[test]
    fn a_held_frame_retires_its_damage_and_asks_for_the_frame_that_gives_it_over() {
        // The renderer's target holds the frame, so its damage is drawn. What is still owed is the
        // copy into a buffer, which the next frame makes once one is free.
        let held = Rotating::flipping().frame(presented());
        assert!(held.retires_damage());
        assert!(held.wants_another_frame());

        let refused = drawn_into_scanout(&Refusing, |slot| {
            assert!(slot.is_none(), "no buffer was taken back");
            (FrameOutcome::Skipped(SkipReason::Held), Vec::new())
        });
        assert_eq!(refused, FrameOutcome::Skipped(SkipReason::Declined));
    }

    #[test]
    fn a_frame_that_composed_nothing_is_not_given_over() {
        // A buffer holds whatever was last copied into it. Giving it over would put that on the
        // screen and report the frame as presented.
        let display = Rotating::new();

        let outcome = display.frame(FrameOutcome::Skipped(SkipReason::Unconfigured));

        assert_eq!(outcome, FrameOutcome::Skipped(SkipReason::Unconfigured));
        assert_eq!(display.steps(), ["acquire 0", "compose into 0"]);
    }

    #[test]
    fn an_undamaged_frame_is_not_put_on_the_screen_a_second_time() {
        // A display on this path has a cursor plane, so a picture that did not change is the one
        // on the screen already.
        let display = Rotating::new();

        let outcome = display.frame(FrameOutcome::Skipped(SkipReason::Undamaged));

        assert_eq!(outcome, FrameOutcome::Skipped(SkipReason::Undamaged));
        assert_eq!(display.steps(), ["acquire 0", "compose into 0"]);
    }

    #[test]
    fn a_frame_that_composed_nothing_leaves_the_next_one_the_buffer_it_took_back() {
        let display = Rotating::new();

        display.frame(FrameOutcome::Recovered);
        assert_eq!(display.frame(presented()), presented());

        assert_eq!(
            display.steps(),
            [
                "acquire 0",
                "compose into 0",
                "acquire 0",
                "compose into 0",
                "give over 0",
            ],
            "a skipped frame moved the display nowhere and left nothing owed"
        );
    }
}
