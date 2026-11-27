//! An animation keeps one frame per refresh on a self-paced driver while the compositor wants a
//! later frame of its own.
//!
//! A slight scale that the window writes every frame keeps the compositor's node moving, and the
//! driver's next frame after each one is the frame at which the node would come to rest, a hold
//! after the last write. The window's commit goes into an earlier frame, the next refresh, and its
//! animation is sampled and paced on that one.

mod support;

use std::time::{Duration, Instant};

use zgui_view::{BuildCx, IntoView, View};

/// A badge that breathes between two slight scales for ever.
const BREATHE_CSS: &str = "root { display: block; width: 400px; height: 300px }
                           .badge { display: block; width: 100px; height: 40px;
                                    background-color: rgb(20, 120, 220);
                                    transform: scale(0.95);
                                    animation: breathe 2s linear infinite }
                           @keyframes breathe {
                               from { transform: scale(0.95) }
                               to { transform: scale(0.97) }
                           }";

/// How long the frames are counted.
const SPAN: Duration = Duration::from_millis(600);

/// Runs the window for `span` of wall time, its clock following the wall clock as the driver's
/// does, and returns how many frames it ran.
fn frames_over(
    harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    span: Duration,
) -> u64 {
    let start = Instant::now();
    let mut last = start;
    let mut frames = 0;
    while last < start + span {
        std::thread::sleep(Duration::from_millis(1));
        let now = Instant::now();
        harness.advance(now - last);
        last = now;
        frames += harness.pump();
    }
    frames
}

#[test]
fn a_slight_scale_animation_draws_a_frame_per_refresh_while_its_node_comes_to_rest_later() {
    let paced = zgui_testkit_compositor::self_paced(support::compositor());
    let mut harness = support::app_with_text_over(paced, BREATHE_CSS, |cx: &mut BuildCx<'_>| {
        let view = zgui_elements::column()
            .class("root")
            .child(zgui_elements::column().class("badge"));
        Box::new(view.into_view().build(cx))
    });
    harness.platform().offscreens()[0].set_refresh_rate_millihertz(Some(60_000));
    frames_over(&mut harness, Duration::from_millis(200));

    let frames = frames_over(&mut harness, SPAN);
    let refreshes = SPAN.as_nanos() / Duration::from_nanos(16_666_667).as_nanos();
    assert!(
        u128::from(frames) * 2 >= refreshes,
        "the animation drew {frames} frames over {refreshes} refreshes"
    );
}
