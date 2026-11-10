//! Real Android lab components: bindings and keyed list identity.
#[path = "../../../examples/android/controls.rs"]
mod controls;
mod harness;
#[path = "../../../examples/android/app.rs"]
mod lab;
#[path = "../../../examples/android/tasks.rs"]
mod tasks;
#[path = "../../../examples/android/visuals.rs"]
mod visuals;

use harness::Harness;
use zgui::prelude::*;

#[test]
fn lab_tabs_keep_task_identity_and_shared_values() {
    let h = Harness::open();
    let state = h.window.scope.with(|| {
        let state = lab::State::new();
        provide_local_context(state);
        state
    });
    h.mount(|| view! { lab::Lab() });
    state.tab.set("tasks".into());
    h.window.frame();
    let rows = h.find_all("task-row");
    assert_eq!(rows.len(), 6);
    state.tasks.update(|tasks| tasks[0].done = true);
    h.window.frame();
    assert!(h.find_all("complete").contains(&rows[0]));
    state.tasks.update(|items| items.reverse());
    h.window.frame();
    assert_eq!(
        h.find_all("task-row"),
        rows.into_iter().rev().collect::<Vec<_>>()
    );
    state.name.set("Pixel explorer".into());
    state.volume.set(72.0);
    state.tab.set("controls".into());
    h.window.frame();
    assert!(state.tasks.get_untracked().last().unwrap().done);
    assert_eq!(state.name.get_untracked(), "Pixel explorer");
    assert_eq!(state.volume.get_untracked(), 72.0);
}

#[test]
fn added_tasks_survive_document_owner_disposal() {
    use zgui::reactive::{Mounted, install};
    install().ok();
    let app_scope = Mounted::new();
    let state = app_scope.with(lab::State::new);
    state.tab.set("tasks".into());
    for generation in 0..2 {
        let h = Harness::open();
        h.window.scope.with(|| provide_local_context(state));
        h.mount(|| view! { lab::Lab() });
        if generation == 0 {
            let button = h
                .all()
                .into_iter()
                .find(|node| h.attribute(*node, "data-testid").as_deref() == Some("add-many"))
                .expect("stress button exists");
            h.click(button);
        }
        assert_eq!(h.find_all("task-row").len(), 56);
        assert_eq!(
            state
                .tasks
                .get_untracked()
                .iter()
                .filter(|task| task.done)
                .count(),
            0
        );
        // Dispose this document before constructing another over same app-owned state.
        h.window.scope.unmount();
    }
    app_scope.unmount();
}
