//! The counter, opened under an application identifier taken from the environment.
//!
//! It exists so that a window can be addressed by a compositor rule that belongs to one measuring
//! run and to nothing else on the desktop. The component and the style sheet are the counter
//! example's, with one addition: a one-pixel border on `:root`, so that the extent the content was
//! laid out into can be read off a capture rather than inferred from where the background stops.
//!
//! Run it with `ZGUI_DIAG_APPID=some-id cargo run --release -p zgui --example resize_diag`.

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
            label(class = "counter__parity") {
                {move || if count.get() % 2 == 0 { "even" } else { "odd" }}
            }
        }
    }
}

/// How it looks: the counter's sheet, plus a marker border around the laid-out viewport.
const SHEET: &str = css!(
    ":root {
        background-color: #12141a;
        color: #e8ecf4;
        font-family: sans-serif;
        display: flex;
        align-items: center;
        justify-content: center;
        border: 2px solid #ff00ff;
    }

    .counter {
        align-items: center;
        gap: 12px;
        padding: 32px 48px;
        border-radius: 16px;
        border: 1px solid #262b36;
        background-color: #191d26;
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

/// A wall of text, for the case where a frame's cost is dominated by glyphs.
#[component]
fn Wall(
    /// How many rows the wall is, which is how a run dials a frame's cost.
    rows: u32,
) -> impl IntoView {
    let rows = (0..rows)
        .map(|row| {
            let text = format!(
                "row {row:02} the quick brown fox jumps over the lazy dog 0123456789 \
                 the quick brown fox jumps over the lazy dog"
            );
            view! { label(class = "wall__row") {{text}} }
        })
        .collect::<Vec<_>>();
    view! { column(class = "wall") {{rows}} }
}

fn main() -> Result<(), zgui::Error> {
    let id = std::env::var("ZGUI_DIAG_APPID").unwrap_or_else(|_| "zgui-diag-resize".to_owned());
    // `ZGUI_DIAG_HEAVY` is how many rows of text the window holds, and so how much one frame
    // costs. Anything that is not a number means the forty rows the name used to stand for.
    let rows: Option<u32> = std::env::var("ZGUI_DIAG_HEAVY")
        .ok()
        .map(|value| value.parse().unwrap_or(40));
    let heavy = rows.is_some();
    let sheet = if heavy {
        format!("{SHEET}\n.wall {{ gap: 2px; }}\n.wall__row {{ font-size: 13px; }}")
    } else {
        SHEET.to_owned()
    };
    let builder = app()
        .with_application_id(id.as_str())
        .with_title(id.as_str())
        .with_size(360.0, 300.0)
        .with_stylesheet(&sheet);
    if heavy {
        let rows = rows.unwrap_or(40);
        builder.run(move || view! { Wall(rows = rows) })
    } else {
        builder.run(|| view! { Counter(start = 0) })
    }
}
