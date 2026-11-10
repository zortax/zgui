//! Keyed list updates, text entry and overflow.
use crate::lab::{State, Task};
use zgui::prelude::*;
use zgui_ui::prelude::*;

#[component]
pub(crate) fn Tasks() -> impl IntoView {
    let s = State::current();
    let add = move || {
        let title = s.draft.get_untracked().trim().to_owned();
        if title.is_empty() {
            return;
        }
        let id = s.next_id.get_untracked();
        s.next_id.set(id + 1);
        s.tasks.update(|tasks| {
            tasks.push(Task {
                id,
                title,
                done: false,
            })
        });
        s.draft.set(String::new());
    };
    view! {
        column(class = "stack") {
            column(class = "card") {
                text(class = "section-title") {"Field checklist"}
                text(class = "muted") {{move || format!("{} of {} complete", s.tasks.get().iter().filter(|t| t.done).count(), s.tasks.get().len())}}
                row(class = "actions") {
                    Input(value = s.draft, label = "New task", placeholder = "Add a check…", class = "task-input", on:key_down = move |ev| {
                        if ev.key == Key::Named(NamedKey::Enter) { add(); }
                    })
                    Button(on:click = move |_| add()) {"Add"}
                }
                row(class = "actions") {
                    Button(variant = ButtonVariant::Outline, attr:data-testid = "add-many", on:click = move |_| {
                        let first = s.next_id.get_untracked();
                        s.next_id.set(first + 50);
                        s.tasks.update(|tasks| tasks.extend((first..first+50).map(|id| Task {
                            id, title: format!("Stress row {}", id + 1), done: false,
                        })));
                    }) {"Add 50"}
                    Button(variant = ButtonVariant::Outline, on:click = move |_| s.tasks.update(|tasks| tasks.retain(|t| !t.done))) {"Clear done"}
                    Button(variant = ButtonVariant::Outline, on:click = move |_| s.tasks.update(|tasks| tasks.reverse())) {"Reverse"}
                }
            }
            ScrollArea(class = "task-scroll") {
                column(class = "task-list") {
                    for task in move || s.tasks.get(), key = |t: &Task| t.id {
                        row(class = "task-row", class:complete = move || s.tasks.get().iter().any(|t| t.id == task.id && t.done)) {
                            Checkbox(
                                checked = Binding::controlled(
                                    Signal::derive_local(move || if s.tasks.get().iter().any(|t| t.id == task.id && t.done) { Checked::Yes } else { Checked::No }),
                                    move |checked| s.tasks.update(|tasks| {
                                        if let Some(item) = tasks.iter_mut().find(|t| t.id == task.id) { item.done = checked == Checked::Yes; }
                                    })
                                ),
                                a11y:label = task.title.clone()
                            )
                            text(class = "task-title") {{task.title.clone()}}
                            Button(variant = ButtonVariant::Ghost, a11y:label = "Remove task", on:click = move |_| s.tasks.update(|tasks| tasks.retain(|t| t.id != task.id))) {"×"}
                        }
                    }
                    if move || s.tasks.get().is_empty() { text(class = "muted") {"All clear. Add a check above."} }
                }
                ScrollBar()
            }
        }
    }
}
