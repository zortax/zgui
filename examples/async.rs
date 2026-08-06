//! Work that takes time: off the UI thread, back on to it, and cancelled when the view goes away.
//!
//! Run it with `cargo run -p zgui-examples --example async`. Press Load to fetch a page of rows,
//! Next/Previous to move between pages, and Cancel to stop a load part way. The bar at the bottom
//! is written by a worker thread that knows nothing about signals.
//!
//! What it is worth reading for:
//!
//! * **one `await` crosses the thread boundary twice.** `background(..)` runs its future on a
//!   worker and resolves on the UI thread, so the whole round trip is written as straight-line
//!   code inside a `spawn`, with `rows.set(..)` on the far side of it — legal because that line
//!   runs on the UI thread, exactly like the line above it;
//! * **the task belongs to the component.** Nothing here registers a cleanup, and nothing has to:
//!   a task spawned from a listener belongs to the owner that listener was written in, so
//!   switching pages mid-load cancels the load that is no longer wanted. The `Task` handle is kept
//!   only because this example also has a Cancel button, which is cancelling *early* rather than
//!   cancelling at all;
//! * **a foreign thread posts rather than writes.** The progress bar's producer is a plain
//!   `std::thread` with no reactive anything in it. It holds a `Ui` handle and posts closures,
//!   which run at the start of the next flush and ask for the frame that shows them;
//! * **the guard against a stale result.** A load that finishes after the user has moved on must
//!   not overwrite what is on screen. The page is re-read after the `await` and the result dropped
//!   if it no longer matches — the one piece of bookkeeping asynchronous code cannot avoid, and
//!   the reason it is written out here rather than hidden.

use std::time::Duration;

use zgui::prelude::*;

/// How many rows a page holds.
const PAGE_SIZE: usize = 8;

/// Stands in for a request: slow, `Send`, and knowing nothing about the interface.
///
/// This is the shape every real one has. It is an ordinary function returning ordinary data, with
/// no framework types anywhere in it, because the only thing that makes it usable from a view is
/// that [`background`] will run it somewhere the frame is not waiting.
fn fetch_page(page: usize) -> Vec<String> {
    std::thread::sleep(Duration::from_millis(900));
    (0..PAGE_SIZE)
        .map(|row| format!("Row {} of page {}", row + 1, page + 1))
        .collect()
}

/// A list that loads a page at a time, and says what it is doing while it does.
#[component]
fn Loader() -> impl IntoView {
    let page = RwSignal::new(0_usize);
    let rows = RwSignal::new(Vec::<String>::new());
    let loading = RwSignal::new(false);
    let progress = RwSignal::new(0_u8);
    // The task in flight, so the Cancel button has something to cancel. Not `LocalStorage`: a
    // `Task` is an `Rc` handle and never leaves this thread, which is what `new_local` is for.
    let in_flight = RwSignal::new_local(None::<Task>);

    let load = move || {
        if loading.get_untracked() {
            return;
        }
        loading.set(true);
        progress.set(0);

        let wanted = page.get_untracked();
        let task = spawn(async move {
            let loaded = background(async move { fetch_page(wanted) }).await;

            // Back on the UI thread. The page may have changed while the worker was busy, and a
            // result for a page nobody is looking at any more is a result to drop.
            if page.get_untracked() == wanted {
                rows.set(loaded);
            }
            loading.set(false);
            in_flight.set(None);
        });
        in_flight.set(Some(task));
    };

    let cancel = move || {
        // Cancelling drops what the task captured there and then. What it does not do is stop the
        // worker: `fetch_page` is a blocking call on another thread with no way to be interrupted,
        // so it runs to the end and its result is thrown away.
        if let Some(task) = in_flight.get_untracked() {
            task.cancel();
        }
        in_flight.set(None);
        loading.set(false);
        progress.set(0);
    };

    let step = move |by: isize| {
        move |_: &mut EventCx<'_, zgui::view::events::Click>| {
            let next = page.get_untracked().saturating_add_signed(by);
            page.set(next);
            rows.set(Vec::new());
            load();
        }
    };

    // A thread with no reactive types in it at all, reporting through a `Ui` handle. Taken here,
    // in the body, because `ui()` answers for the thread it is called on.
    let ui = ui();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(60));
            ui.post(move || {
                if loading.get_untracked() {
                    progress.update(|done| *done = (*done + 4).min(96));
                }
            });
        }
    });

    view! {
        column(class = "loader", a11y:role = Role::Group, a11y:label = "Paged loader") {
            row(class = "loader__bar") {
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    a11y:label = "Previous page",
                    on:click = step(-1)
                ) { "Previous" }
                label(class = "loader__page") {
                    {move || format!("Page {}", page.get() + 1)}
                }
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    a11y:label = "Next page",
                    on:click = step(1)
                ) { "Next" }
                control(
                    class = "button button--primary",
                    tabindex = Focus::Sequential,
                    on:click = move |_| load()
                ) { "Load" }
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    on:click = move |_| cancel()
                ) { "Cancel" }
            }

            if move || loading.get() {
                column(class = "loader__status") {
                    label(class = "loader__pending") {"Loading…"}
                    row(class = "meter") {
                        text(class = "meter__fill", style:width = move || {
                            Some(format!("{}%", progress.get()))
                        }) {""}
                    }
                }
            } else {
                label(class = "loader__status loader__idle") {
                    {move || match rows.get().len() {
                        0 => "Nothing loaded yet.".to_string(),
                        n => format!("{n} rows"),
                    }}
                }
            }

            column(class = "list") {
                for row in move || rows.get(), key = |row: &String| row.clone() {
                    label(class = "list__row") {{row}}
                }
            }
        }
    }
}

const SHEET: &str = css!(
    ":root { background: #14161a; color: #f2f4f8; font-family: sans-serif }
     .loader { gap: 14px; padding: 24px }
     .loader__bar { gap: 8px; align-items: center }
     .loader__page { min-width: 84px; font-weight: 600 }
     .button { padding: 6px 14px; border-radius: 8px; background: #262a33 }
     .button--primary { background: #2b6cff }
     .loader__status { gap: 8px; min-height: 34px }
     .loader__idle { color: #97a0b0 }
     .loader__pending { color: #97a0b0 }
     .meter { height: 6px; border-radius: 3px; background: #262a33 }
     .meter__fill { height: 6px; border-radius: 3px; background: #2b6cff }
     .list { gap: 4px }
     .list__row { padding: 6px 10px; border-radius: 6px; background: #1b1e25 }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Async")
        .with_title("Async")
        .with_size(460.0, 520.0)
        .with_stylesheet(SHEET)
        .run(|| view! { Loader() })
}
