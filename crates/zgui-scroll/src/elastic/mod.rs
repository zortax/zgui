//! What happens to a delta no container could absorb.
//!
//! Content dragged past its end follows the gesture with diminishing returns and springs back when
//! the gesture ends. That says "this is the end" without a message, and the displacement past the
//! end is also the progress of a pull-to-refresh gesture.
//!
//! The displacement is a separate quantity from the scroll offset. An offset is clamped to what the
//! content allows, because the scrollbar thumb, `scrollTop` and a virtualiser's observation all
//! describe content that exists. The displacement is composed on top at paint time.
//!
//! # A finger holds the edge, the frame clock returns it
//!
//! While a touchpad gesture has its fingers down, the edge is *gripped*: it stays where the finger
//! put it and no return runs. When the fingers lift, the return runs on the frame clock.
//! [`Scroller::is_animating`](crate::Scroller::is_animating) counts an edge that returns, so the
//! park installs a deadline for it. A gripped edge does not move, so it asks for no frames.

mod resist;
mod spring;

use core::time::Duration;

use zgui_geom::{Device, DevicePx, Size};

use crate::elastic::spring::Return;

/// One container's displacement past its end, and how it returns.
///
/// ```
/// use core::time::Duration;
/// use zgui_geom::{Device, DevicePx, Size};
/// use zgui_scroll::elastic::Overscroll;
///
/// let port = Size::<DevicePx, Device>::new(DevicePx(400.0), DevicePx(600.0));
/// let pulled = Overscroll::default().pulled_by(
///     Size::new(DevicePx(0.0), DevicePx(100.0)),
///     port,
/// );
/// assert!(pulled.held().height.0 > 0.0, "it follows the gesture");
/// assert!(pulled.held().height.0 < 100.0, "with resistance");
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
    /// The horizontal displacement and its return.
    across: Return,
    /// The vertical displacement and its return.
    down: Return,
    /// Whether a finger holds the edge where it is.
    gripped: bool,
}

impl Overscroll {
    /// How far past its end the content is drawn.
    pub fn held(self) -> Size<DevicePx, Device> {
        Size::new(DevicePx(self.across.at()), DevicePx(self.down.at()))
    }

    /// Whether the edge is back at its end and no longer moving.
    pub fn arrived(self) -> bool {
        self.across.arrived() && self.down.arrived()
    }

    /// Whether a finger holds the edge.
    pub fn is_gripped(self) -> bool {
        self.gripped
    }

    /// The same edge, held where it is by a finger.
    pub fn gripped(self) -> Self {
        Self {
            across: Return::start(self.across.at(), 0.0),
            down: Return::start(self.down.at(), 0.0),
            gripped: true,
        }
    }

    /// The same edge, let go so that it returns.
    pub fn released(self) -> Self {
        Self {
            gripped: false,
            ..self.gripped()
        }
    }

    /// The same edge, thrown at `speed` device pixels per second on each axis that has one.
    ///
    /// This is a momentum scroll that reaches the end: the edge travels on past it, then returns.
    pub fn thrown(self, speed: Size<DevicePx, Device>) -> Self {
        let throw = |edge: Return, speed: f32| {
            if speed == 0.0 {
                edge
            } else {
                Return::start(edge.at(), speed)
            }
        };
        Self {
            across: throw(self.across, speed.width.0),
            down: throw(self.down, speed.height.0),
            gripped: false,
        }
    }

    /// The displacement after `unabsorbed` has been pulled into it, against a scrollport of
    /// `extent`.
    ///
    /// The band makes each further pixel of pull move the content less than the one before, so no
    /// pull drags the content a full scrollport. An axis with no pull keeps its return.
    pub fn pulled_by(
        self,
        unabsorbed: Size<DevicePx, Device>,
        extent: Size<DevicePx, Device>,
    ) -> Self {
        let pull = |edge: Return, by: f32, extent: f32| {
            if by == 0.0 {
                return edge;
            }
            let held = resist::band(resist::unband(edge.at(), extent) + by, extent);
            Return::start(held, 0.0)
        };
        Self {
            across: pull(self.across, unabsorbed.width.0, extent.width.0),
            down: pull(self.down, unabsorbed.height.0, extent.height.0),
            gripped: self.gripped,
        }
    }

    /// The displacement after `delta` has pulled it back towards its end, and what is left of
    /// `delta` once it is there.
    ///
    /// A finger that moves back first undoes the stretch, then scrolls the content. An axis with
    /// no displacement, or one that `delta` pulls further out, passes its whole delta on.
    pub fn unwound_by(
        self,
        delta: Size<DevicePx, Device>,
        extent: Size<DevicePx, Device>,
    ) -> (Self, Size<DevicePx, Device>) {
        let unwind = |edge: Return, by: f32, extent: f32| {
            let held = edge.at();
            if held == 0.0 || by == 0.0 || held.signum() == by.signum() {
                return (edge, by);
            }
            let pulled = resist::unband(held, extent) + by;
            if pulled.signum() == held.signum() {
                (Return::start(resist::band(pulled, extent), 0.0), 0.0)
            } else {
                (Return::default(), pulled)
            }
        };
        let (across, left_x) = unwind(self.across, delta.width.0, extent.width.0);
        let (down, left_y) = unwind(self.down, delta.height.0, extent.height.0);
        (
            Self {
                across,
                down,
                gripped: self.gripped,
            },
            Size::new(DevicePx(left_x), DevicePx(left_y)),
        )
    }

    /// The same displacement measured on a device with `by` times as many pixels per CSS pixel.
    ///
    /// The displacement and the thrown speed are both in device pixels, so both scale. The return
    /// then takes the same time as before.
    pub fn scaled(self, by: f32) -> Self {
        Self {
            across: self.across.scaled(by),
            down: self.down.scaled(by),
            gripped: self.gripped,
        }
    }

    /// The displacement after `elapsed` of its return. A gripped edge does not move.
    pub fn advanced(self, elapsed: Duration) -> Self {
        if self.gripped {
            return self;
        }
        let seconds = elapsed.as_secs_f32();
        Self {
            across: self.across.advanced(seconds),
            down: self.down.advanced(seconds),
            gripped: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_geom::{Device, DevicePx, Size};

    use super::Overscroll;

    /// A scrollport 600 device pixels tall.
    const PORT: Size<DevicePx, Device> = Size::new(DevicePx(400.0), DevicePx(600.0));

    fn down(by: f32) -> Size<DevicePx, Device> {
        Size::new(DevicePx(0.0), DevicePx(by))
    }

    #[test]
    fn a_displacement_that_was_never_made_is_already_arrived() {
        assert!(Overscroll::default().arrived());
        assert!(!Overscroll::default().pulled_by(down(1.0), PORT).arrived());
    }

    #[test]
    fn the_port_is_never_left_however_hard_it_is_pulled() {
        let mut edge = Overscroll::default();
        for _ in 0..200 {
            edge = edge.pulled_by(down(50.0), PORT);
        }
        assert!(edge.held().height.0 < PORT.height.0);
    }

    #[test]
    fn a_gripped_edge_stays_where_the_finger_put_it() {
        let edge = Overscroll::default().pulled_by(down(100.0), PORT).gripped();
        let later = edge.advanced(Duration::from_secs(1));
        assert_eq!(later.held(), edge.held());
        assert!(!later.arrived());
        let mut let_go = later.released();
        for _ in 0..60 {
            let_go = let_go.advanced(Duration::from_millis(16));
        }
        assert!(let_go.arrived());
    }

    #[test]
    fn moving_back_undoes_the_stretch_before_anything_else() {
        let edge = Overscroll::default()
            .pulled_by(down(-100.0), PORT)
            .gripped();
        let (partly, left) = edge.unwound_by(down(40.0), PORT);
        assert_eq!(left, down(0.0));
        assert!(partly.held().height.0 < 0.0 && partly.held().height.0 > edge.held().height.0);

        let (home, left) = edge.unwound_by(down(130.0), PORT);
        assert_eq!(home.held().height.0, 0.0);
        assert!((left.height.0 - 30.0).abs() < 0.01, "{left:?}");
    }

    #[test]
    fn a_pull_further_out_is_passed_on_to_the_band() {
        let edge = Overscroll::default().pulled_by(down(-100.0), PORT);
        let (same, left) = edge.unwound_by(down(-10.0), PORT);
        assert_eq!(same, edge);
        assert_eq!(left, down(-10.0));
    }

    #[test]
    fn a_throw_travels_past_the_end_and_comes_back() {
        let mut edge = Overscroll::default().thrown(down(3000.0));
        let mut peak: f32 = 0.0;
        for _ in 0..120 {
            edge = edge.advanced(Duration::from_millis(8));
            peak = peak.max(edge.held().height.0);
        }
        assert!(peak > 10.0, "{peak}");
        assert!(edge.arrived());
    }

    #[test]
    fn each_axis_returns_on_its_own() {
        let edge = Overscroll::default().pulled_by(Size::new(DevicePx(40.0), DevicePx(0.0)), PORT);
        assert!(edge.held().width.0 > 0.0);
        assert_eq!(edge.held().height.0, 0.0);
        let mut edge = edge;
        for _ in 0..60 {
            edge = edge.advanced(Duration::from_millis(16));
        }
        assert!(edge.arrived());
    }
}
