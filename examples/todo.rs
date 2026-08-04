//! A todo list: a keyed collection, text from the keyboard, and state that outlives a row.
//!
//! Run it with `cargo run -p zgui-examples --example todo`. Type to write an item, press Enter to
//! add it, click an item to tick it off, and click its `x` to remove it.
//!
//! It also has the inspector wired in, which is what an application does to get one: press
//! <kbd>F12</kbd> for the panel, <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> to pick an element,
//! <kbd>F8</kbd> to freeze it. The Tree tab shows this file's own components against the lines they
//! are written on.
//!
//! What it is worth reading for:
//!
//! * **the list is keyed.** `for … in …, key = …` re-runs its reconciliation when the *set* of
//!   items changes, and moves the rows it already has rather than rewriting them. A row's own
//!   content follows that row's own reads, so ticking one item touches one row;
//! * **the count is derived, not stored.** Nothing remembers how many are left and nothing has to
//!   invalidate it;
//! * **`if` is a branch, not a hidden element.** The empty state holds no nodes at all while
//!   there are items;
//! * **the draft is assembled from key presses.** This framework has no text-editing control yet,
//!   so the example does the small part of one it needs, which is also the clearest look at how a
//!   listener's payload is typed: `ev.key` in an `on:key_down` handler is a `Key`, with no
//!   downcast anywhere, and `Key::inserted_text` is what turns a press into the text it means.

use zgui::prelude::*;
#[allow(
    unused_imports,
    reason = "the tag names the component and the macro names its props type"
)]
use zgui_devtools::{DevTools, Inspector, InspectorProps};

/// One item of the list.
///
/// Whether an item is done is a signal of its own rather than a plain `bool`, and that is the
/// whole difference between a list that redraws itself and one that redraws a row: the collection
/// changes when an item is added or removed, and a row's own tick changes only that row's signal,
/// which only that row's bindings read.
#[derive(Clone, Debug)]
struct Todo {
    /// What identifies this item for as long as it exists.
    id: u64,
    /// What it says.
    label: String,
    /// Whether it has been done.
    done: RwSignal<bool>,
}

/// The list, the draft being typed, and one row per item.
#[component]
fn Todos() -> impl IntoView {
    let items = RwSignal::new(Vec::<Todo>::new());
    let draft = RwSignal::new(String::new());
    let next_id = RwSignal::new(1_u64);

    // Adding is written once and used from two places, which is the ordinary reason to lift a
    // closure out of a view.
    let add = move || {
        let label = draft.get_untracked().trim().to_owned();
        if label.is_empty() {
            return;
        }
        let id = next_id.get_untracked();
        next_id.set(id + 1);
        items.update(|items| {
            items.push(Todo {
                id,
                label,
                done: RwSignal::new(false),
            })
        });
        draft.set(String::new());
    };

    let remaining = move || items.get().iter().filter(|item| !item.done.get()).count();

    view! {
        column(
            class = "todos",
            tabindex = Focus::Sequential,
            a11y:role = Role::Group,
            a11y:label = "Todo list",
            on:key_down = move |ev| match &ev.key {
                Key::Named(NamedKey::Backspace) => draft.update(|draft| { draft.pop(); }),
                Key::Named(NamedKey::Enter) => add(),
                // Every other key is asked what text it inserts, which is the only reading that
                // gets the space bar right: it is a *named* key whose text is one space.
                key => if let Some(text) = key.inserted_text() {
                    draft.update(|draft| draft.push_str(text));
                },
            }
        ) {
            label(class = "todos__title") {"Todo"}

            row(class = "draft", a11y:role = Role::TextInput) {
                text(class = "draft__text") {{move || draft.get()}}
                text(class = "draft__caret") {"|"}
                spacer()
                text(class = "draft__hint") {"type, then Enter"}
            }

            column(class = "list") {
                for item in move || items.get(), key = |item: &Todo| item.id {
                    row(
                        class = "todo",
                        class:done = move || item.done.get(),
                        on:click = move |_| item.done.update(|done| *done = !*done)
                    ) {
                        text(class = "todo__mark") {
                            {move || if item.done.get() { "*" } else { "-" }}
                        }
                        text(class = "todo__label") {{item.label.clone()}}
                        spacer()
                        control(
                            class = "todo__remove",
                            a11y:label = "Remove",
                            on:click:stop = move |_| items.update(|items| {
                                items.retain(|other| other.id != item.id);
                            })
                        ) {
                            "x"
                        }
                    }
                }
            }

            if move || items.get().is_empty() {
                label(class = "todos__empty") {"Nothing to do yet."}
            } else {
                label(class = "todos__count") {{move || format!("{} left", remaining())}}
            }
        }
    }
}

/// How it looks.
const SHEET: &str = css!(
    ":root {
        background-color: #0f1117;
        color: #e6eaf2;
        font-family: sans-serif;
        display: flex;
        justify-content: center;
        padding: 40px 0;
    }

    .todos {
        width: 420px;
        gap: 14px;
        padding: 24px;
        border-radius: 14px;
        border: 1px solid #232a38;
        background-color: #161a23;
        box-shadow: 0 16px 36px rgba(0, 0, 0, 0.4);
    }

    .todos__title {
        font-size: 22px;
        font-weight: 700;
    }

    .draft {
        align-items: center;
        gap: 2px;
        padding: 10px 12px;
        border-radius: 10px;
        border: 1px solid #2a3242;
        background-color: #10141c;
    }

    .draft__text { font-size: 15px; }
    .draft__caret { font-size: 15px; color: #4d7bff; }
    .draft__hint { font-size: 12px; color: #6b7689; }

    .list { gap: 6px; }

    .todo {
        align-items: center;
        gap: 10px;
        padding: 8px 12px;
        border-radius: 8px;
        background-color: #1b212c;
    }

    .todo:hover { background-color: #212836; }

    .todo__mark { color: #4d7bff; font-weight: 700; }
    .todo__label { font-size: 15px; }

    .todo.done .todo__label {
        color: #6b7689;
        text-decoration: line-through;
    }

    .todo__remove {
        padding: 2px 8px;
        border-radius: 6px;
        color: #6b7689;
        font-size: 14px;
    }

    .todo__remove:hover { color: #ff6b6b; background-color: #2a1d24; }

    .todos__count, .todos__empty {
        font-size: 13px;
        color: #6b7689;
    }"
);

fn main() -> Result<(), zgui::Error> {
    // Two lines: a view to draw the panel in, and a probe to read each frame through. The panel
    // installs its own style sheet, so `SHEET` below is this application's and nothing else's.
    let tools = DevTools::new();
    app()
        .with_application_id("dev.zgui.Todo")
        .with_title("Todo")
        .with_size(520.0, 520.0)
        .with_stylesheet(SHEET)
        .with_probe(tools.probe())
        .run(move || view! { Inspector(tools = tools) {Todos()} })
}
