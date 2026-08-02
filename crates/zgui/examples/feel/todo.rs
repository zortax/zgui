//! The todo example's view and style sheet, copied from `examples/todo.rs` unchanged.
//!
//! The one interface in the shipped set with a text field, which is what makes it the one a
//! keystroke's latency can be measured through.

#![allow(dead_code)]

use zgui::prelude::*;

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
pub(crate) fn Todos() -> impl IntoView {
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
pub(crate) const SHEET: &str = css!(
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
