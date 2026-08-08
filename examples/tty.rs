//! A clock on a bare Linux console: no display server, no compositor, no window.
//!
//! **This one needs the device.** The console backend takes DRM master and holds it, so it needs a
//! free virtual terminal or root, and it refuses to start while a compositor is holding the card.
//! Switch to a spare terminal with `Ctrl+Alt+F3`, log in, and run:
//!
//! ```text
//! cargo build -p zgui-examples --example tty --features drm
//! ./target/debug/examples/tty
//! ```
//!
//! Every display that is plugged in is lit at its own preferred mode. Stop it with `Ctrl+C`.
//!
//! **There is no input.** Reading the evdev devices is a sub-project this backend has not started,
//! so nothing here can be pressed, pointed at or typed into. The example is a clock for that
//! reason: it moves on its own, which is the only thing an application on this backend can do.
//!
//! What it is worth reading for:
//!
//! * **the application is an ordinary one.** Everything below the `main` is written against
//!   `zgui::prelude::*` and says nothing about the kernel, the mode or the flip. The one line that
//!   knows where this runs is `run_drm`;
//! * **a timer paces it.** Nothing spins. The interval asks for a frame once a second, the loop
//!   sleeps in `poll` in between, and a console with nothing moving on it costs no processor time
//!   at all;
//! * **the seconds bar is the proof.** A clock that only shows a time could be a still picture with
//!   the right time on it. The bar fills one segment per second and empties on the minute, which is
//!   something no still frame can be.

use std::time::Duration;

use zgui::prelude::*;

/// How often the clock is asked for the time.
///
/// A second: what is on the screen changes once a second, so a frame more often than that would
/// draw the same picture again.
const TICK: Duration = Duration::from_secs(1);

/// How many segments the seconds bar has.
const SEGMENTS: u64 = 60;

/// A clock that counts from the moment the program started.
///
/// The elapsed time rather than the time of day, because a console has no locale, no time zone
/// database and nothing to say what a person here reads a date as. What a running program can state
/// truthfully is how long it has been running.
#[component]
fn Clock() -> impl IntoView {
    let elapsed = RwSignal::new(0_u64);
    // The handle cancels the interval when it goes, so it is kept for as long as this component is.
    // `new_local` because a timer handle never leaves the thread the window runs on.
    let _tick = RwSignal::new_local(set_interval(TICK, move || {
        elapsed.update(|seconds| *seconds += 1);
    }));

    view! {
        column(class = "clock", a11y:role = Role::Group, a11y:label = "Running time") {
            label(class = "clock__caption") {"RUNNING FOR"}
            text(class = "clock__time") {{move || written(elapsed.get())}}
            row(class = "clock__bar") {
                // Built once and never rebuilt. Each segment reads the same signal and lights
                // itself, so a tick writes the two segments that changed rather than sixty.
                for segment in || 0..SEGMENTS, key = |segment: &u64| *segment {
                    column(
                        class = "seg",
                        class:seg-lit = move || segment < elapsed.get() % SEGMENTS
                    )
                }
            }
            label(class = "clock__note") {"no display server, no window, no input"}
        }
    }
}

/// `seconds` as hours, minutes and seconds.
fn written(seconds: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3_600,
        (seconds / 60) % 60,
        seconds % 60
    )
}

/// How it looks.
///
/// A console shows whatever the framebuffer holds and nothing else, so the background here is the
/// whole screen rather than a window on one.
const SHEET: &str = css!(
    ":root {
        background-color: #05070c;
        color: #e8ecf4;
        font-family: sans-serif;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .clock {
        align-items: center;
        gap: 24px;
        padding: 48px 72px;
        border-radius: 20px;
        border: 1px solid #1b2230;
        background-color: #0b0f18;
    }

    .clock__caption {
        font-size: 15px;
        letter-spacing: 6px;
        color: #55627a;
    }

    .clock__time {
        font-size: 120px;
        font-weight: 700;
        line-height: 1.05;
    }

    .clock__bar {
        gap: 3px;
        height: 18px;
    }

    .seg {
        width: 8px;
        height: 18px;
        border-radius: 2px;
        background-color: #1b2230;
    }

    .seg-lit { background-color: #3b6cf6; }

    .clock__note {
        font-size: 14px;
        letter-spacing: 2px;
        color: #55627a;
    }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Tty")
        .with_title("zgui on a console")
        .with_stylesheet(SHEET)
        .run_drm(|| view! { Clock() })
}
