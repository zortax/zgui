//! A clock on a bare Linux console: no display server, no compositor, no window.
//!
//! **This one needs the device.** Switch to a spare terminal with `Ctrl+Alt+F3`, log in, and run:
//!
//! ```text
//! cargo build -p zgui-examples --example tty --features drm
//! ./target/debug/examples/tty
//! ```
//!
//! Where a session daemon answers, it opens the card for this program and no privilege of its own
//! is needed. Where none answers, the backend opens the card and takes DRM master itself, which
//! needs root or a terminal nothing else holds. It refuses to start while a compositor holds the
//! card.
//!
//! Every display that is plugged in is lit at its own preferred mode.
//!
//! **Switching away from it is not handled yet.** `Ctrl+Alt+F1` does nothing here: a session daemon
//! turns the console keyboard off while it holds the devices, and a run with no daemon grabs every
//! keyboard for itself. A switch made some other way takes the devices while this program goes on
//! running.
//!
//! **Press Escape to stop it, and read the next paragraph before running it.** The backend takes
//! the keyboard away from everything else, so `Ctrl+C` never reaches the terminal's line discipline
//! and raises no `SIGINT`. The backend binds no key of its own — which key leaves a program is the
//! program's decision, and one chosen by a backend is one taken away from every application that
//! wanted it for something else — so an application on a console has to bind one, and `Clock` binds
//! Escape. A build of this without that binding has to be killed from another terminal.
//!
//! What it is worth reading for:
//!
//! * **the application is an ordinary one.** Everything below the `main` is written against
//!   `zgui::prelude::*` and says nothing about the kernel, the mode, the flip or the keyboard. The
//!   one line that knows where this runs is `run_drm`;
//! * **a timer paces it.** Nothing spins. The interval asks for a frame once a second, the loop
//!   sleeps in `poll` in between, and a console with nothing moving on it costs no processor time
//!   at all;
//! * **the seconds bar is the proof.** A clock that only shows a time could be a still picture with
//!   the right time on it. The bar fills one segment per second and empties on the minute, which is
//!   something no still frame can be;
//! * **the key that leaves is a window shortcut**, and it has to be. A key is delivered along the
//!   path to whatever holds focus, and a window in which nothing holds focus routes one to the
//!   document's root and no further — so a listener on the column below would hear nothing at all
//!   until something has been focused;
//! * **the pointer is the same pointer.** The two controls are ordinary `on:click` listeners with
//!   ordinary `:hover` rules over them, and the wheel scrolls the list below them because the list
//!   overflows. Nothing about any of it is written for a console;
//! * **a device plugged in while it runs is picked up.** Unplug the mouse and the cursor stops;
//!   plug it in again and it moves, with the program still running. A key or a button the vanished
//!   device was holding is let go, so a button lit under a finger goes out rather than staying lit
//!   for the rest of the run.
//!
//! The pointer costs one thing worth knowing in advance. The mouse is grabbed the way the keyboard
//! is, so it stops reaching whatever else was reading it for as long as this runs. The cursor goes
//! on a hardware plane where the device has one — every real card — and is drawn into the frame
//! where it does not, which is every virtual machine.

use std::time::Duration;

use zgui::prelude::*;
use zgui::reactive::RenderEffect;

/// How often the clock is asked for the time.
///
/// A second: what is on the screen changes once a second, so a frame more often than that would
/// draw the same picture again.
const TICK: Duration = Duration::from_secs(1);

/// How many segments the seconds bar has.
const SEGMENTS: u64 = 60;

/// How often the caret changes state.
///
/// Half a second on and half a second off is the rate every text field blinks at, and it is slow
/// enough that the frames it costs on an otherwise idle console are two a second.
const BLINK: Duration = Duration::from_millis(500);

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

    // What hears a key on a window in which nothing has focus. The registration is renewed whenever
    // the element it names is bound, and the guard is dropped with the scope, so nothing outlives
    // the document.
    let anchor = NodeRef::new();
    let registration = RenderEffect::new(move |previous: Option<Option<WindowShortcut>>| {
        drop(previous);
        anchor.get();
        anchor.window_shortcut()
    });
    on_cleanup_local(move || drop(registration));

    // What has been typed into this window, and where the document was last told the pointer is.
    // Neither is a control: a console has no text field element, and a text field anywhere else is
    // a caret drawn beside a string.
    let typed = RwSignal::new(String::new());
    let at = RwSignal::new(None::<(f32, f32)>);
    // How far the wheel has asked to scroll altogether. A reading that stands still while the wheel
    // turns says no turn reached the document at all, which is a different fault from a turn that
    // reached it and scrolled nothing.
    let turned = RwSignal::new(0.0_f32);

    // A caret that stands still looks like a program that has stopped. This one is a signal read by
    // the glyph's own class, so a blink writes one attribute rather than rebuilding the row.
    let lit = RwSignal::new(true);
    let _blink = RwSignal::new_local(set_interval(BLINK, move || {
        lit.update(|lit| *lit = !*lit);
    }));

    let windows = use_windows();
    let window = use_window();

    view! {
        column(
            class = "clock",
            node_ref = anchor,
            // The only way out. See the note at the top of this file: the keyboard is grabbed, so
            // no `SIGINT` is raised and a program that binds nothing here cannot be stopped from
            // the terminal it is running on.
            on:key_down = move |ev| match &ev.key {
                Key::Named(NamedKey::Escape) => windows.quit(),
                Key::Named(NamedKey::Backspace) => typed.update(|typed| {
                    typed.pop();
                }),
                // Every other key is asked what text it inserts, which is the only reading that
                // gets the space bar right: it is a *named* key whose text is one space. A held
                // key arrives again as a repeat, and a repeat inserts text the way a press does.
                key => {
                    if let Some(text) = key.inserted_text() {
                        // A chord such as `Ctrl+A` inserts text too — the control character the
                        // layout says that chord means. A field with no notion of selection has
                        // nothing to do with one, and putting it in the string would show as a
                        // hole, so what cannot be read is left out.
                        typed.update(|typed| {
                            typed.extend(text.chars().filter(|glyph| !glyph.is_control()));
                        });
                    }
                }
            },
            // Where the pointer is, as the document was told. This separates a pointer that does
            // not move from a cursor that does not follow one: the reading changes for the second
            // and stands still for the first.
            // Written once and never again. A reading that follows the pointer damages text on
            // every motion, and a frame here is a readback off the GPU, a copy into the buffer the
            // display scans out of, and a flip — so a live reading paces the cursor by the slowest
            // thing on the machine and measures itself rather than the pointer.
            on:pointer_move = move |ev| {
                if at.get_untracked().is_none() {
                    at.set(Some((ev.position.x.0, ev.position.y.0)));
                }
            },
            // Counted here rather than on the list, so that a turn is recorded wherever the pointer
            // was. A turn that arrives and scrolls nothing is a scroll that did not reach what it
            // was over; a turn that never arrives is a wheel this backend did not read.
            on:wheel = move |ev| {
                if let ScrollDelta::Lines { y, .. } = ev.delta {
                    turned.update(|turned| *turned += y);
                }
            },
            a11y:role = Role::Group,
            a11y:label = "Running time"
        ) {
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
            row(class = "clock__buttons") {
                // What the pointer is over decides the shape it takes, and an application says so:
                // a cursor is not a style this engine reads out of a sheet. The console backend
                // draws each shape itself, because a machine with no display server has no cursor
                // theme to read one from.
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    a11y:label = "Back a minute",
                    on:pointer_enter = {
                        let window = window.clone();
                        move |_| window.set_cursor(CursorStyle::Pointer)
                    },
                    on:pointer_leave = {
                        let window = window.clone();
                        move |_| window.set_cursor(CursorStyle::Default)
                    },
                    on:click = move |_| {
                        elapsed.update(|seconds| *seconds = seconds.saturating_sub(60));
                    }
                ) {
                    "-1 min"
                }
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    a11y:label = "Start again",
                    on:pointer_enter = {
                        let window = window.clone();
                        move |_| window.set_cursor(CursorStyle::Pointer)
                    },
                    on:pointer_leave = {
                        let window = window.clone();
                        move |_| window.set_cursor(CursorStyle::Default)
                    },
                    on:click = move |_| elapsed.set(0)
                ) {
                    "reset"
                }
            }
            // Taller than the box it is in, so the wheel has somewhere to go. A detent leaves the
            // backend as a detent and the framework decides how far one travels, which is why this
            // scrolls by three lines here and on a desktop alike.
            label(class = "log__caption") {"SCROLL THIS BOX"}
            column(class = "log", a11y:role = Role::Group, a11y:label = "Ticks") {
                for segment in || 0..SEGMENTS, key = |segment: &u64| *segment {
                    label(class = "log__line") {{move || format!("tick {segment:02}")}}
                }
            }
            // A text field is a string and a caret. `inserted_text` fills it, so holding a key
            // repeats into it and a dead key followed by a letter arrives as one composed character
            // rather than two.
            row(class = "field", a11y:role = Role::TextInput, a11y:label = "Type here") {
                text(class = "field__text") {{move || typed.get()}}
                text(class = "field__caret", class:field__caret-lit = move || lit.get()) {"|"}
                spacer()
                text(class = "field__hint") {"type — backspace deletes"}
            }
            // The pointer as the document heard about it. A reading that stands still while the
            // mouse moves says the pointer never arrived; a reading that moves under a cursor that
            // does not says the pointer arrived and the plane was never told.
            label(class = "clock__note") {{move || {
                let reached = match at.get() {
                    Some((x, y)) => format!("pointer reached {x:.0}, {y:.0}"),
                    None => "pointer has not moved".to_owned(),
                };
                format!("{reached} · wheel {:+.1} — ESC to leave", turned.get())
            }}}
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

    .clock__buttons { gap: 12px; }

    .button {
        padding: 8px 18px;
        border-radius: 10px;
        border: 1px solid #1b2230;
        background-color: #141a26;
        color: #e8ecf4;
        font-size: 15px;
    }

    /* Read by the pointer and by the keyboard alike, which is what a console has to prove: the
       same rule lights under a hover and under a tab. */
    .button:hover { background-color: #2b3243; }
    .button:active { background-color: #3b6cf6; }
    .button:focus-visible { border-color: #3b6cf6; }

    .log {
        width: 260px;
        height: 96px;
        overflow: auto;
        padding: 6px 10px;
        border-radius: 10px;
        border: 1px solid #1b2230;
        background-color: #090d15;
    }

    .log__caption {
        font-size: 12px;
        letter-spacing: 4px;
        color: #55627a;
    }

    .log__line {
        font-size: 13px;
        color: #55627a;
    }

    .clock__note {
        font-size: 14px;
        letter-spacing: 2px;
        color: #55627a;
    }

    .field {
        align-items: center;
        gap: 2px;
        width: 420px;
        padding: 10px 14px;
        border-radius: 8px;
        border: 1px solid #1b2230;
        background-color: #05070c;
    }

    .field__text { font-size: 16px; }
    .field__caret { font-size: 16px; color: #05070c; }
    .field__caret-lit { color: #3b6cf6; }

    .field__hint {
        font-size: 13px;
        letter-spacing: 1px;
        color: #55627a;
    }"
);

/// Sends the log to a file, because on this console standard error is the screen.
///
/// Temporary, and the reason this example names a second crate: the console takes the display
/// over, so a message written to standard error lands on pixels the frame loop is about to
/// overwrite. `ZGUI_TTY_LOG` names the file; the default suits a machine with a writable `/tmp`.
///
/// Every level is recorded. The decision a display makes between scanning out of the buffers the
/// renderer draws into and copying each frame through the CPU is an `info!`, and it is the first
/// thing to read here.
fn log() {
    let path = std::env::var("ZGUI_TTY_LOG").unwrap_or_else(|_| "/tmp/zgui-tty.log".to_owned());
    let Ok(file) = std::fs::File::create(&path) else {
        return;
    };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(file))
        .with_ansi(false)
        .with_max_level(tracing::Level::TRACE)
        .finish();
    drop(tracing::subscriber::set_global_default(subscriber));
}

fn main() -> Result<(), zgui::Error> {
    log();
    let described = app()
        .with_application_id("dev.zgui.Tty")
        .with_title("zgui on a console")
        .with_stylesheet(SHEET);

    // The same application in a window, for telling this backend's faults from the framework's.
    // A behaviour that shows here and under a desktop belongs to `zgui`; one that shows only on
    // the console belongs to the console backend. Temporary, and it is why `ZGUI_TTY_WINDOWED`
    // has no documentation anywhere a person would look for it.
    if std::env::var_os("ZGUI_TTY_WINDOWED").is_some() {
        return described.run(|| view! { Clock() });
    }
    described.run_drm(|| view! { Clock() })
}
