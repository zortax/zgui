//! Several windows: opening them, closing them, and the state they share.
//!
//! Run it with `cargo run -p zgui-examples --example windows`.
//!
//! What it is worth reading for:
//!
//! * `use_windows().open(…)` opens a window from anywhere — here from a listener, while a frame is
//!   running — and answers with a handle before the window exists;
//! * a signal read by two windows is written once and both are redrawn, because a signal belongs to
//!   the application rather than to the window that made it;
//! * `use_window()` resolves the window a component is running *in*, so the same component reports
//!   its own size and renames its own title wherever it is mounted;
//! * `on_close_request` refuses a close, which is what a document with unsaved work does while it
//!   asks;
//! * every window operation is a plain call with no platform branch around it: what this desktop
//!   cannot do, it quietly does not do.

use zgui::prelude::*;

/// The count both windows read and write.
#[derive(Clone, Copy)]
struct Shared(RwSignal<i32>);

/// The window the application launches with.
#[component]
fn Main() -> impl IntoView {
    let shared = expect_context::<Shared>().0;
    let windows = use_windows();
    let this = use_window();
    // How many children have been opened, so each gets a name of its own.
    let opened = RwSignal::new(0);

    // Reactive holes are written before the view rather than inside it: a braced child is one
    // expression, and each of these needs a clone of the handle it reads.
    let geometry = {
        let this = this.clone();
        move || {
            let size = this.size().get();
            format!(
                "{:.0} x {:.0} at {:.1}x",
                size.width.0,
                size.height.0,
                this.scale().get()
            )
        }
    };
    let count_of_windows = {
        let windows = windows.clone();
        move || format!("{} window(s) open", windows.watch().get().len())
    };

    view! {
        column(class = "page") {
            label(class = "eyebrow") {"Main window"}
            text(class = "count") {{move || shared.get().to_string()}}
            label(class = "hint") {"Shared by every window. Change it anywhere."}

            row(class = "buttons") {
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    on:click = move |_| shared.update(|n| *n -= 1)
                ) {"-"}
                control(
                    class = "button button--primary",
                    tabindex = Focus::Sequential,
                    on:click = move |_| shared.update(|n| *n += 1)
                ) {"+"}
            }

            row(class = "buttons") {
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    // Opened from inside a frame. The window appears on the next turn of the loop.
                    on:click = {
                        let windows = windows.clone();
                        move |_| {
                            let number = opened.get_untracked() + 1;
                            opened.set(number);
                            windows.open(
                                WindowOptions::new(format!("Child {number}"))
                                    .with_size(320.0, 260.0)
                                    .with_stylesheet(CHILD_SHEET),
                                move || view! { Child(number = number) },
                            );
                        }
                    }
                ) {"Open a window"}
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    on:click = {
                        let windows = windows.clone();
                        let this = this.clone();
                        move |_| {
                            // Every window except the one this is running in.
                            for window in windows.all() {
                                if window.id() != this.id() {
                                    window.close();
                                }
                            }
                        }
                    }
                ) {"Close the others"}
            }

            // What this window is, read reactively off its own handle.
            label(class = "readout") {{geometry}}
            label(class = "readout") {{count_of_windows}}

            row(class = "buttons") {
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    on:click = {
                        let this = this.clone();
                        move |_| this.request_size(560.0, 620.0)
                    }
                ) {"Resize me"}
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    on:click = {
                        let this = this.clone();
                        move |_| this.toggle_maximized()
                    }
                ) {"Maximise"}
            }
        }
    }
}

/// A window opened by the main one.
#[component]
fn Child(
    /// Which child this is.
    number: i32,
) -> impl IntoView {
    let shared = expect_context::<Shared>().0;
    let this = use_window();
    let focus_state = {
        let this = this.clone();
        move || {
            if this.focused().get() {
                "focused"
            } else {
                "not focused"
            }
        }
    };
    // Refuses the first close and relents on the second, as an unsaved document would.
    let asked = RwSignal::new(false);
    let guard = on_close_request(move || {
        if asked.get_untracked() {
            CloseResponse::Close
        } else {
            asked.set(true);
            CloseResponse::Veto
        }
    });
    // Held for as long as the question is worth asking, which here is the window's whole life.
    core::mem::forget(guard);

    view! {
        column(class = "page") {
            label(class = "eyebrow") {{format!("Child {number}")}}
            // The same signal the main window shows. Nothing connects the two but the signal.
            text(class = "count") {{move || shared.get().to_string()}}
            row(class = "buttons") {
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    on:click = move |_| shared.update(|n| *n += 1)
                ) {"+1 from here"}
            }
            label(class = "hint") {
                {move || if asked.get() {
                    "Close it again and it will go."
                } else {
                    "Closing this window is refused once."
                }}
            }
            label(class = "readout") {{focus_state}}
            row(class = "buttons") {
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    on:click = {
                        let this = this.clone();
                        move |_| this.close()
                    }
                ) {"Close me"}
            }
        }
    }
}

/// What both windows look like.
const SHEET: &str = css!(
    ":root {
        background-color: #12141a;
        color: #e8ecf4;
        font-family: sans-serif;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .page {
        align-items: center;
        gap: 10px;
        padding: 28px 36px;
    }

    .eyebrow {
        font-size: 12px;
        letter-spacing: 2px;
        color: #7d879b;
    }

    .count {
        font-size: 56px;
        font-weight: 700;
        line-height: 1.1;
    }

    .hint, .readout {
        font-size: 12px;
        color: #7d879b;
        text-align: center;
    }

    .buttons { gap: 10px; }

    .button {
        padding: 9px 18px;
        border-radius: 10px;
        border: 1px solid #2f3646;
        background-color: #232936;
        color: #e8ecf4;
        font-size: 15px;
        line-height: 1;
        text-align: center;
    }

    .button:hover { background-color: #2b3243; }

    .button--primary {
        background-color: #3b6cf6;
        border-color: #3b6cf6;
    }

    .button--primary:hover { background-color: #4d7bff; }"
);

/// What a child window adds to the application's own sheet, and nothing more.
///
/// A window's sheet is cascaded *after* the application's, so this changes the background of the
/// windows it is given to and leaves everything else where the application put it.
const CHILD_SHEET: &str = css!(":root { background-color: #171a22 }");

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Windows")
        .with_title("Windows")
        .with_size(420.0, 520.0)
        .with_stylesheet(SHEET)
        // Above every window rather than inside the first one's view: a context provided in a
        // window is that window's own, and a child window would resolve nothing for it.
        .with_context(|| provide_context(Shared(RwSignal::new(0))))
        .run(|| view! { Main() })
}
