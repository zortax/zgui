//! Scrolls the compositor animates on its own.

use core::time::Duration;

use compositor_types::{
    Animation, AnimationStart, AnimationTarget, Direction, Easing, FillMode, Keyframe, Keyframes,
    ScrollId, Timing,
};

use crate::sync::CompositorSync;

impl CompositorSync {
    /// Makes the compositor move the scroll node `scroll` from `from` to `to`, in device pixels,
    /// over `duration` along `curve`, starting at its next frame.
    ///
    /// `curve` holds `[progress, eased progress]` points with non-decreasing progress. The node
    /// stays at `to` once the animation ends. Nothing is written when no transaction is open.
    pub fn animate_scroll(
        &mut self,
        scroll: ScrollId,
        [from, to]: [[f32; 2]; 2],
        duration: Duration,
        curve: &[[f32; 2]],
    ) {
        let Some(transaction) = &mut self.transaction else {
            return;
        };
        let keyframe = |offset: f32, value: [f32; 2]| Keyframe {
            offset,
            value,
            easing: Easing::LinearPoints(curve.into()),
        };
        transaction.animate(Animation {
            target: AnimationTarget::ScrollOffset(scroll),
            keyframes: Keyframes::Vector(smallvec::smallvec![
                keyframe(0.0, from),
                keyframe(1.0, to)
            ]),
            timing: Timing {
                duration_ns: u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
                delay_ns: 0,
                iterations: 1.0,
                direction: Direction::Normal,
                fill: FillMode::Forwards,
                easing: Easing::Linear,
            },
            start: AnimationStart::Next,
        });
    }
}
