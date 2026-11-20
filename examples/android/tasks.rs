//! Keyed list updates, text entry and overflow.
use crate::lab::{State, Task};
use zgui::prelude::*;
use zgui_ui::prelude::*;
#[path = "tasks/list.rs"]
mod list;
use list::TaskRowsProps;

#[component]
pub(crate) fn Tasks() -> impl IntoView {
    let s = State::current();
    let count = Signal::derive_local(move || s.tasks.with(Vec::len));
    // How far the list has been pulled down from its top, in CSS pixels.
    let pull = RwSignal::new_local(0.0_f32);
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
                text(class = "muted") {{move || format!("{} of {} complete", s.tasks.with(|tasks| tasks.iter().filter(|t| t.done).count()), count.get())}}
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
            if move || pull.get() > 0.0 {
                text(class = "pull", style:height = move || Some(format!("{}px", pull.get().min(list::PULL_REFRESH)))) {
                    {move || if pull.get() >= list::PULL_REFRESH { "Release to sort open checks first" } else { "Pull to sort" }}
                }
            }
            ScrollArea(class = "task-scroll", label = "Checklist") {
                TaskRows(pull = pull)
            }
        }
    }
}
