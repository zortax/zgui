//! The counter: the smallest application that has state.
//!
//! Run it with `cargo run -p zgui-examples --example counter`.
//!
//! What it is worth reading for:
//!
//! * a signal is the whole state model — there is no store, no reducer and no context;
//! * a closure in a view is a reactive hole, and it is one because of its *type*: `{move || …}`
//!   is written again whenever what it reads changes, and `{…}` without the closure is written
//!   once and never again;
//! * `on:click` is a real listener, taking part in capture and bubble, and its argument's type is
//!   inferred from the event's name;
//! * the appearance is ordinary CSS, checked where it is written.

use zgui::prelude::*;

/// A number, and two buttons that change it.
#[component]
fn Counter(
    /// Where the count starts.
    #[prop(default = 0)]
    start: i32,
) -> impl IntoView {
    let (count, set_count) = signal(start);

    view! {
        column(class = "counter", a11y:role = Role::Group, a11y:label = "Counter") {
            label(class = "counter__caption") {"Count"}
            text(class = "counter__value") {{move || count.get().to_string()}}
            row(class = "counter__buttons") {
                control(
                    class = "button",
                    tabindex = Focus::Sequential,
                    a11y:label = "Decrease",
                    on:click = move |_| set_count.update(|n| *n -= 1)
                ) {
                    "-"
                }
                control(
                    class = "button button--primary",
                    tabindex = Focus::Sequential,
                    a11y:label = "Increase",
                    on:click = move |_| set_count.update(|n| *n += 1)
                ) {
                    "+"
                }
            }
            // A second reader of the same signal. Nothing wires the two together: both read the
            // count, so both are re-run when it changes, and nothing else in the window is.
            label(class = "counter__parity") {
                {move || if count.get() % 2 == 0 { "even" } else { "odd" }}
            }
        }
    }
}

/// How it looks.
const SHEET: &str = css!(
    ":root {
        background-color: #12141a;
        color: #e8ecf4;
        font-family: sans-serif;
        display: flex;
        align-items: center;
        justify-content: center;
    }

    .counter {
        align-items: center;
        gap: 12px;
        padding: 32px 48px;
        border-radius: 16px;
        border: 1px solid #262b36;
        background-color: #191d26;
        box-shadow: 0 18px 40px rgba(0, 0, 0, 0.45);
    }

    .counter__caption {
        font-size: 13px;
        letter-spacing: 2px;
        color: #7d879b;
    }

    .counter__value {
        font-size: 64px;
        font-weight: 700;
        line-height: 1.1;
    }

    .counter__parity {
        font-size: 13px;
        color: #7d879b;
    }

    .counter__buttons { gap: 12px; }

    .button {
        padding: 10px 22px;
        border-radius: 10px;
        border: 1px solid #2f3646;
        background-color: #232936;
        color: #e8ecf4;
        font-size: 20px;
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

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Counter")
        .with_title("Counter")
        .with_size(360.0, 300.0)
        .with_stylesheet(SHEET)
        .run(|| view! { Counter(start = 0) })
}
