//! A tally of named counters: add a row, count it up, clear the ones that reached zero.
//!
//! Written against `zgui::prelude::*` and nothing else, to find out what an application actually
//! needs and whether one import supplies it.

use zgui::prelude::*;

/// One named row, and its own count.
#[derive(Clone)]
struct Row {
    /// What distinguishes this row from another with the same name.
    id: usize,
    /// What the row is called.
    name: String,
    /// How many, as a signal of its own so a row updates without its siblings.
    count: RwSignal<i32>,
}

/// One row of the tally.
#[component]
fn Tally<R: Fn(usize) + 'static>(
    /// The row to show.
    row: Row,
    /// Called with the row's id when its remove button is pressed.
    on_remove: R,
) -> impl IntoView {
    let count = row.count;
    let id = row.id;
    view! {
        row(class = "row") {
            label(class = "row__name") {{row.name.clone()}}
            label(class = "row__count") {{move || count.get().to_string()}}
            control(
                class = "chip",
                tabindex = Focus::Sequential,
                a11y:label = "Increment",
                on:click = move |_| count.update(|n| *n += 1)
            ) {
                "+"
            }
            control(
                class = "chip",
                tabindex = Focus::Sequential,
                a11y:label = "Remove",
                on:click = move |_| on_remove(id)
            ) {
                "x"
            }
        }
    }
}

/// The whole application.
#[component]
fn Tallies() -> impl IntoView {
    let (rows, set_rows) = signal(vec![
        Row {
            id: 0,
            name: "apples".to_string(),
            count: RwSignal::new(2),
        },
        Row {
            id: 1,
            name: "pears".to_string(),
            count: RwSignal::new(0),
        },
    ]);
    let (next, set_next) = signal(2usize);

    // Derived, not stored: the total is a function of the rows and never a third thing to keep in
    // step with them.
    let total = move || rows.get().iter().map(|row| row.count.get()).sum::<i32>();

    let remove = move |id: usize| set_rows.update(|rows| rows.retain(|row| row.id != id));

    view! {
        column(class = "page", a11y:role = Role::Group, a11y:label = "Tally") {
            row(class = "head") {
                label(class = "title") {"Tally"}
                label(class = "total") {{move || format!("{} total", total())}}
            }
            for row in move || rows.get(), key = |row: &Row| row.id {
                Tally(row = row, on_remove = remove)
            }
            if move || rows.get().is_empty() {
                label(class = "empty") {"nothing counted yet"}
            }
            control(
                class = "chip chip--wide",
                tabindex = Focus::Sequential,
                a11y:label = "Add a row",
                on:click = move |_| {
                    let id = next.get();
                    set_next.set(id + 1);
                    set_rows.update(|rows| {
                        rows.push(Row {
                            id,
                            name: format!("row {id}"),
                            count: RwSignal::new(0),
                        });
                    });
                }
            ) {
                "add a row"
            }
        }
    }
}

/// How it looks.
const SHEET: &str = css!(
    ":root {
        background-color: #0f1116;
        color: #e6e9ef;
        font-family: sans-serif;
        padding: 24px;
    }

    .page { gap: 10px; }

    .head { align-items: baseline; gap: 12px; }

    .title { font-size: 24px; font-weight: 700; }

    .total { font-size: 13px; color: #808a9d; }

    .row {
        align-items: center;
        gap: 10px;
        padding: 8px 12px;
        border-radius: 10px;
        background-color: #171b24;
    }

    .row__name { width: 120px; }

    .row__count {
        width: 40px;
        text-align: right;
        font-weight: 700;
    }

    .empty { color: #808a9d; font-size: 13px; }

    .chip {
        padding: 6px 12px;
        border-radius: 8px;
        border: 1px solid #2b3242;
        background-color: #212736;
        text-align: center;
    }

    .chip:hover { background-color: #2b3242; }

    .chip--wide { padding: 8px 16px; }"
);

fn main() -> Result<(), zgui::Error> {
    app()
        .with_application_id("dev.zgui.Tally")
        .with_title("Tally")
        .with_size(420.0, 420.0)
        .with_stylesheet(SHEET)
        .run(|| view! { Tallies() })
}
