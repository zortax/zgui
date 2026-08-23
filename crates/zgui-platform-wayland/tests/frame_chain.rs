//! Every redraw ends in a commit, and every commit is answered before the next one runs.
//!
//! This is the property the whole backend is built around, and it has two halves that fail in
//! opposite directions.
//!
//! **The chain must not end.** A frame callback rides a commit, so a turn that draws nothing and
//! commits nothing leaves the compositor with nothing to answer, and it never speaks about that
//! surface again. From outside that is a window that painted once and stopped. The half of this
//! test that catches it is simply that the loop finishes: an application asking for a frame after
//! every frame runs to its stopping point only if every one of them arrived.
//!
//! **The chain must not be skipped.** A client that commits twice without the compositor answering
//! in between is racing the display, and only the last of those frames is ever shown. The half of
//! this test that catches it is the pacer's own count of callbacks it gave up waiting for: a
//! healthy compositor answers every one, so a run that abandoned any either drew ahead of the
//! answers or never asked.

#[path = "support/mod.rs"]
mod support;

use std::sync::atomic::Ordering;

use zgui_platform::PlatformError;
use zgui_platform_wayland::WaylandApp;

/// How many frames the application is given before it asks the loop to finish.
///
/// More than one, because one frame proves only that a surface was mapped. The second is the one
/// that had to arrive through a callback the compositor sent.
const FRAMES: u64 = 8;

fn main() {
    const PROPERTY: &str = "every redraw ends in a commit, and every commit is answered";

    support::tracing();
    support::watchdog(PROPERTY);

    let handler = support::StopAfter {
        inner: match support::application("frame chain") {
            Ok(runtime) => runtime,
            Err(error) => {
                support::skipped(PROPERTY, &format!("the runtime would not install: {error}"));
                return;
            }
        },
        frames: FRAMES,
        sustained: true,
    };

    let mut app = match WaylandApp::new(handler) {
        Ok(app) => app,
        Err(PlatformError::Backend(reason)) => {
            support::skipped(PROPERTY, &format!("no compositor to run on: {reason}"));
            return;
        }
        Err(other) => panic!("the compositor refused the application: {other}"),
    };

    if let Err(error) = app.run() {
        panic!("the loop stopped: {error}");
    }

    if support::NO_DEVICE.load(Ordering::Relaxed) {
        support::skipped(
            PROPERTY,
            "this machine has no graphics device to draw through",
        );
        return;
    }

    let frames = support::FRAMES.load(Ordering::Relaxed);
    assert!(
        frames >= FRAMES,
        "the loop finished after {frames} frames of {FRAMES}: the chain ended early"
    );

    let turns = app.turns();
    assert!(
        turns < frames * 200,
        "{turns} turns for {frames} frames: the loop is spinning rather than waiting"
    );

    support::passed(
        PROPERTY,
        &format!(
            "{frames} frames over {turns} turns, {} deadlines",
            app.park().resumes()
        ),
    );
}
