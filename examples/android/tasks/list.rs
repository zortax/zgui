//! Visible checklist rows, inside ScrollArea's stable clipped viewport.
use crate::lab::{Haptic, State, Task, haptic, notify};
use zgui::prelude::*;
use zgui_ui::prelude::*;
use zgui_ui::virtualize::Virtualize;

/// Downward pull at the list's top, in CSS pixels, that sorts open checks first.
pub(super) const PULL_REFRESH: f32 = 80.0;

/// Horizontal travel, in CSS pixels, that deletes a swiped row.
const SWIPE_DELETE: f32 = 120.0;

/// One row: tap to check, swipe sideways to delete, long-press for a menu.
#[component]
fn TaskRow(task: Task) -> impl IntoView {
    let s = State::current();
    let toasts = use_toaster();
    let id = task.id;
    let done = move || {
        s.tasks
            .with(|tasks| tasks.iter().any(|t| t.id == id && t.done))
    };
    let set_done = move |value: bool| {
        s.tasks.update(|tasks| {
            if let Some(item) = tasks.iter_mut().find(|t| t.id == id) {
                item.done = value;
            }
        });
        if !value {
            return;
        }
        haptic(Haptic::Confirm);
        let total = s.tasks.with_untracked(Vec::len);
        if s.enabled.get_untracked() && s.tasks.with_untracked(|tasks| tasks.iter().all(|t| t.done))
        {
            notify(
                toasts,
                "Checklist complete",
                &format!("All {total} checks done."),
                std::time::Duration::ZERO,
            );
        }
    };
    let remove = move || {
        haptic(Haptic::Confirm);
        s.tasks.update(|tasks| tasks.retain(|t| t.id != id));
    };
    // Finger start position, and horizontal offset while swiping.
    let start = RwSignal::new_local(None::<(f32, f32)>);
    let swiping = RwSignal::new_local(false);
    let offset = RwSignal::new_local(0.0_f32);
    let title = task.title.clone();
    let label = task.title.clone();
    let shared = task.title;
    let share = UnsyncCallback::new(move |()| {
        #[cfg(target_os = "android")]
        zgui::app::android::share(&shared);
        #[cfg(not(target_os = "android"))]
        let _ = &shared;
    });
    view! {
        ContextMenu {
            ContextMenuTrigger(class = "task-menu", a11y:role = Role::ListItem) {
                row(
                    class = "task-row",
                    attr:data-task-id = id.to_string(),
                    class:complete = done,
                    style:transform = move || Some(format!("translateX({}px)", offset.get())),
                    style:opacity = move || Some(format!("{}", 1.0 - (offset.get().abs() / (SWIPE_DELETE * 2.0)).min(0.6))),
                    on:pointer_down = move |ev| {
                        if (**ev).kind == zgui::vocab::PointerKind::Touch {
                            start.set(Some((ev.position.x.0, ev.position.y.0)));
                        }
                    },
                    on:pointer_move = move |ev| {
                        let Some((x, y)) = start.get_untracked() else { return };
                        let (dx, dy) = (ev.position.x.0 - x, ev.position.y.0 - y);
                        if !swiping.get_untracked() {
                            if dx.abs().max(dy.abs()) < 6.0 {
                                return;
                            }
                            // Vertical travel belongs to the list's scroll.
                            if dy.abs() > dx.abs() {
                                start.set(None);
                                return;
                            }
                            // Captured, so the scroller does not take the finger.
                            ev.capture_pointer();
                            swiping.set(true);
                        }
                        if (offset.get_untracked().abs() < SWIPE_DELETE) != (dx.abs() < SWIPE_DELETE) {
                            haptic(Haptic::Tick);
                        }
                        offset.set(dx);
                    },
                    on:pointer_up = move |ev| {
                        ev.release_pointer();
                        swiping.set(false);
                        start.set(None);
                        if offset.get_untracked().abs() >= SWIPE_DELETE {
                            remove();
                        } else {
                            offset.set(0.0);
                        }
                    },
                    on:pointer_cancel = move |ev| {
                        ev.release_pointer();
                        swiping.set(false);
                        start.set(None);
                        offset.set(0.0);
                    }
                ) {
                    Checkbox(
                        checked = Binding::controlled(
                            Signal::derive_local(move || if done() { Checked::Yes } else { Checked::No }),
                            move |checked| set_done(checked == Checked::Yes)
                        ),
                        a11y:label = title.clone()
                    )
                    text(class = "task-title") {{label.clone()}}
                    Button(variant = ButtonVariant::Ghost, a11y:label = "Remove task", on:click = move |_| remove()) {"×"}
                }
            }
            ContextMenuContent {
                MenuItem(on_select = UnsyncCallback::new(move |()| set_done(!done()))) {
                    {move || if done() { "Mark not done" } else { "Mark done" }}
                }
                MenuItem(on_select = share) {"Share"}
                MenuSeparator()
                MenuItem(destructive = true, on_select = UnsyncCallback::new(move |()| remove())) {"Delete"}
            }
        }
    }
}

#[component]
pub(super) fn TaskRows(pull: RwSignal<f32, zgui::reactive::LocalStorage>) -> impl IntoView {
    let s = State::current();
    let toasts = use_toaster();
    let area = ScrollAreaContext::current().expect("rows live inside ScrollArea");
    let viewport = area.viewport();
    // Where a touch at the list's top started, while it may still become a pull.
    let pulling = RwSignal::new_local(None::<f32>);
    let release = move || {
        pulling.set(None);
        if pull.get_untracked() >= PULL_REFRESH {
            s.tasks.update(|tasks| tasks.sort_by_key(|t| t.done));
            haptic(Haptic::Confirm);
            if let Some(toasts) = toasts {
                toasts.push(Toast::new("Sorted: open checks first"));
            }
        }
        pull.set(0.0);
    };
    let count = Signal::derive_local(move || s.tasks.with(Vec::len));
    // Match fixed CSS row height; scrolling mounts visible rows plus four rows of slack.
    let visible = Virtualize::new(viewport, count, 60.0, 4);
    view! {
        column(
            class = "task-list",
            a11y:role = Role::List,
            on:pointer_down = move |ev| {
                if (**ev).kind == zgui::vocab::PointerKind::Touch && area.position().offset.y.0 <= 0.0 {
                    pulling.set(Some(ev.position.y.0));
                }
            },
            on:pointer_move = move |ev| {
                let Some(from) = pulling.get_untracked() else { return };
                let next = (ev.position.y.0 - from).max(0.0);
                if pull.get_untracked() == 0.0 {
                    if next < 8.0 {
                        // An upward drag scrolls the list instead.
                        if ev.position.y.0 < from - 8.0 {
                            pulling.set(None);
                        }
                        return;
                    }
                    // Captured, so the scroller does not stretch under the pull.
                    ev.capture_pointer();
                }
                if (pull.get_untracked() < PULL_REFRESH) != (next < PULL_REFRESH) {
                    haptic(Haptic::Tick);
                }
                pull.set(next);
            },
            on:pointer_up = move |ev| { ev.release_pointer(); release() },
            on:pointer_cancel = move |ev| { ev.release_pointer(); pulling.set(None); pull.set(0.0); },
            style:padding-top = move || Some(format!("{}px", visible.window().lead)),
            style:padding-bottom = move || Some(format!("{}px", visible.window().trail))
        ) {
            for task in move || {
                let window = visible.window();
                s.tasks.with(|tasks| tasks[window.first.min(tasks.len())..window.end().min(tasks.len())].to_vec())
            }, key = |t: &Task| t.id {
                TaskRow(task = task)
            }
            if move || count.get() == 0 { text(class = "muted") {"All clear. Add a check above."} }
        }
    }
}
