//! What happens to a delta no container could absorb.
//!
//! Content dragged past its end does not stop dead: it follows the gesture with diminishing
//! returns and springs back when the gesture ends. That is the affordance that says "this is the
//! end" without a message, and it is also the reason a pull-to-refresh gesture is expressible at
//! all — the displacement past the end *is* the gesture's progress.
//!
//! The displacement is deliberately not a scroll offset. An offset is clamped to what the content
//! allows, because everything downstream of it — the scrollbar thumb, `scrollTop`, the observation
//! a virtualiser reads — is a statement about content that exists. The elastic displacement is a
//! separate, transient quantity composed on top at paint time and sprung away over the following
//! frames.
//!
//! # It is an animation, and it is driven by the frame clock
//!
//! The return carries a *speed* as well as a position, and both are advanced by however long the
//! last frame took. That is what distinguishes it from a decay evaluated whenever an event happens
//! to arrive, and the difference is visible: an edge whose position is a function of event arrival
//! moves in the bursts the events arrive in, so a wheel held against the bottom of a list stutters
//! — the edge is yanked out, released from a standstill, and yanked out again, at whatever rate the
//! mouse reports. Here a detent adds to a spring that is already moving and the spring keeps
//! running at the refresh rate, so what is seen is one continuous stretch and one continuous
//! return.
//!
//! [`Scroller::is_animating`](crate::Scroller::is_animating) counts a displacement that has not yet
//! come back, so the park installs a deadline for it exactly as it does for a smooth scroll. Without
//! that the edge would stay stretched until something else happened to ask for a frame.

mod resist;
mod spring;

use core::time::Duration;

use zgui_geom::{Device, DevicePx, Size};

/// How far past its end a container may be dragged, in device pixels.
///
/// Pulling harder past this stops moving anything rather than dragging the content off the screen.
pub const BAND: f32 = resist::BAND;

/// One container's displacement past its end, and how fast it is coming back.
///
/// The speed is state and not a derived quantity, which is the whole of why this is a type rather
/// than a pair of free functions over a displacement. A spring's position at the next frame is a
/// function of where it is *and how fast it is going*, and a return that recomputed its speed from
/// its position each frame is a return whose shape changes with the frame rate.
///
/// ```
/// use core::time::Duration;
/// use zgui_geom::{Device, DevicePx, Size};
/// use zgui_scroll::elastic::Overscroll;
///
/// let pulled = Overscroll::default().pulled_by(Size::<DevicePx, Device>::new(
///     DevicePx(0.0),
///     DevicePx(100.0),
/// ));
/// assert!(pulled.held().height.0 > 0.0, "it follows the gesture");
/// assert!(pulled.held().height.0 < 100.0, "with resistance");
/// assert!(!pulled.arrived());
///
/// // And comes back on its own, given frames.
/// let mut edge = pulled;
/// for _ in 0..60 {
///     edge = edge.advanced(Duration::from_millis(16));
/// }
/// assert!(edge.arrived());
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Overscroll {
    /// How far past its end the content is being drawn.
    held: Size<DevicePx, Device>,
    /// How fast that is changing, in device pixels per second.
    speed: Size<DevicePx, Device>,
}

impl Overscroll {
    /// How far past its end the content is being drawn.
    pub fn held(self) -> Size<DevicePx, Device> {
        self.held
    }

    /// Whether the edge is back where it belongs and no longer moving.
    pub fn arrived(self) -> bool {
        self.held.width.0 == 0.0
            && self.held.height.0 == 0.0
            && self.speed.width.0 == 0.0
            && self.speed.height.0 == 0.0
    }

    /// The displacement after `unabsorbed` has been pulled into it.
    ///
    /// The resistance is what makes the band finite: each further pixel of gesture moves the
    /// content less than the one before it, asymptotically to the edge of the band. So dragging
    /// twice as hard does not go twice as far, and no gesture, however long, drags the content out
    /// of the window.
    ///
    /// The speed is left where it was rather than zeroed. A detent that arrives while the edge is
    /// already returning adds to a moving spring, which is what makes a run of detents against the
    /// bottom of a list one continuous stretch instead of a series of jerks.
    pub fn pulled_by(self, unabsorbed: Size<DevicePx, Device>) -> Self {
        Self {
            held: Size::new(
                DevicePx(resist::resist(self.held.width.0, unabsorbed.width.0)),
                DevicePx(resist::resist(self.held.height.0, unabsorbed.height.0)),
            ),
            speed: self.speed,
        }
    }

    /// The same displacement measured on a device with `by` times as many pixels per CSS pixel.
    ///
    /// Both halves are device-pixel quantities — a displacement and a speed in device pixels per
    /// second — so a surface that changed its ratio has changed the number that stands for the same
    /// physical stretch. Scaling the speed with the displacement is what keeps the return the same
    /// length of time: a spring whose position was rescaled and whose velocity was not is one that
    /// crawls back at double the ratio and snaps back at half it.
    pub fn scaled(self, by: f32) -> Self {
        Self {
            held: Size::new(
                DevicePx(self.held.width.0 * by),
                DevicePx(self.held.height.0 * by),
            ),
            speed: Size::new(
                DevicePx(self.speed.width.0 * by),
                DevicePx(self.speed.height.0 * by),
            ),
        }
    }

    /// The displacement after `elapsed` of the spring.
    pub fn advanced(self, elapsed: Duration) -> Self {
        let (width, speed_x) = spring::advance(self.held.width.0, self.speed.width.0, elapsed);
        let (height, speed_y) = spring::advance(self.held.height.0, self.speed.height.0, elapsed);
        Self {
            held: Size::new(DevicePx(width), DevicePx(height)),
            speed: Size::new(DevicePx(speed_x), DevicePx(speed_y)),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_geom::{Device, DevicePx, Size};

    use super::{BAND, Overscroll};

    fn down(by: f32) -> Size<DevicePx, Device> {
        Size::new(DevicePx(0.0), DevicePx(by))
    }

    #[test]
    fn a_displacement_that_was_never_made_is_already_arrived() {
        assert!(Overscroll::default().arrived());
        assert!(!Overscroll::default().pulled_by(down(1.0)).arrived());
    }

    #[test]
    fn the_band_is_never_left_however_hard_it_is_pulled() {
        let mut edge = Overscroll::default();
        for _ in 0..200 {
            edge = edge.pulled_by(down(50.0));
        }
        assert!(
            edge.held().height.0 < BAND,
            "ten thousand pixels of gesture dragged the content {} past its end",
            edge.held().height.0
        );
    }

    #[test]
    fn a_pull_during_a_return_keeps_the_speed_the_return_had() {
        // The stutter this replaces: an edge whose speed is thrown away at every arriving detent
        // restarts its return from a standstill each time, so a wheel held against the bottom of a
        // list produces one visible jerk per detent instead of one continuous stretch.
        let moving = Overscroll::default()
            .pulled_by(down(100.0))
            .advanced(Duration::from_millis(16));
        assert!(
            moving.advanced(Duration::ZERO) == moving,
            "no time passing must move nothing"
        );
        let pulled = moving.pulled_by(down(10.0));
        assert!(pulled.held().height.0 > moving.held().height.0);
        assert!(
            pulled.advanced(Duration::from_millis(16)).held().height.0 < pulled.held().height.0,
            "the spring did not carry on returning after the pull"
        );
    }

    #[test]
    fn a_pull_in_the_other_direction_displaces_the_other_way() {
        let up = Overscroll::default().pulled_by(down(-40.0));
        assert!(up.held().height.0 < 0.0);
        assert!(up.pulled_by(down(40.0)).held().height.0.abs() < 1e-3);
    }

    #[test]
    fn each_axis_springs_back_on_its_own() {
        let edge = Overscroll::default().pulled_by(Size::new(DevicePx(40.0), DevicePx(0.0)));
        assert!(edge.held().width.0 > 0.0);
        assert_eq!(edge.held().height.0, 0.0);
        let mut edge = edge;
        for _ in 0..60 {
            edge = edge.advanced(Duration::from_millis(16));
        }
        assert!(edge.arrived());
    }
}
