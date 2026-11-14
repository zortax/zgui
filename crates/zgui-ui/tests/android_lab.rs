//! Real Android lab components: bindings and keyed list identity.
#[path = "../../../examples/android/controls.rs"]
mod controls;
mod harness;
#[path = "../../../examples/android/app.rs"]
mod lab;
#[path = "../../../examples/android/phone.rs"]
mod phone;
#[path = "../../../examples/android/tasks.rs"]
mod tasks;
#[path = "../../../examples/android/visuals.rs"]
mod visuals;
#[path = "../../../examples/android/widgets.rs"]
mod widgets;

#[path = "../../../examples/android/services.rs"]
mod services;
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
        assert_eq!(state.tasks.get_untracked().len(), 56);
        assert!(
            h.find_all("task-row").len() <= 16,
            "only visible rows should mount"
        );
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

#[test]
fn thousands_of_tasks_mount_only_visible_rows_and_scroll_to_correct_ids() {
    use zgui::geom::{DevicePx, Point, Size};
    use zgui::view::ObservedValue;
    let h = Harness::open();
    let state = h.window.scope.with(|| {
        let state = lab::State::new();
        state.tasks.set(
            (0..5_000)
                .map(|id| lab::Task {
                    id,
                    title: format!("Task {id}"),
                    done: false,
                })
                .collect(),
        );
        state.tab.set("tasks".into());
        provide_local_context(state);
        state
    });
    h.mount(|| view! { lab::Lab() });
    let scroll = h.find("zui-scroll-area__viewport");
    let position = |offset| ScrollPosition {
        offset: Point::new(DevicePx(0.0), DevicePx(offset)),
        content_size: Size::new(DevicePx(350.0), DevicePx(300_000.0)),
        scrollport: Size::new(DevicePx(350.0), DevicePx(330.0)),
    };
    h.window
        .dom
        .deliver(scroll, ObservedValue::ScrollPosition(position(0.0)));
    h.window.frame();
    assert!(h.find_all("task-row").len() <= 16);
    h.window
        .dom
        .deliver(scroll, ObservedValue::ScrollPosition(position(6_000.0)));
    h.window.frame();
    let rows = h.find_all("task-row");
    assert!(rows.len() <= 16);
    assert_eq!(h.attribute(rows[0], "data-task-id").as_deref(), Some("96"));
    // Updates to an offscreen task must appear when its row enters view.
    state.tasks.update(|tasks| tasks[100].done = true);
    h.window.frame();
    assert_eq!(h.find_all("complete").len(), 1);
    assert_eq!(
        h.attribute(h.find("complete"), "data-task-id").as_deref(),
        Some("100")
    );
}

#[test]
fn android_back_closes_demo_dialog() {
    let h = Harness::open();
    let state = h.window.scope.with(|| {
        let state = lab::State::new();
        provide_local_context(state);
        state
    });
    h.mount(|| view! { lab::Lab() });
    state.dialog.set(true);
    h.window.frame();
    let dialog = h.find("zui-dialog");
    h.press(dialog, zgui::vocab::NamedKey::BrowserBack);
    assert!(
        !state.dialog.get_untracked(),
        "Back must dismiss demo dialog"
    );
}

#[test]
fn service_actions_report_platform_requirement_and_keep_manual_verification() {
    let h = Harness::open();
    let state = h.window.scope.with(|| {
        let state = lab::State::new();
        state.tab.set("services".into());
        provide_local_context(state);
        state
    });
    h.mount(|| view! { lab::Lab() });
    let action = h
        .all()
        .into_iter()
        .find(|node| h.attribute(*node, "data-service").as_deref() == Some("download"))
        .expect("download action mounted");
    h.click(action);
    assert_eq!(
        state.service_report.get_untracked(),
        "download: Android device required"
    );
    assert!(
        state.service_checks.get_untracked().is_empty(),
        "launching must not claim verification"
    );
    let check = h
        .all()
        .into_iter()
        .find(|node| h.attribute(*node, "data-verified").as_deref() == Some("download"))
        .expect("verification checkbox mounted");
    h.click(check);
    assert_eq!(state.service_checks.get_untracked(), vec!["download"]);
    state.tab.set("controls".into());
    h.window.frame();
    state.tab.set("services".into());
    h.window.frame();
    assert_eq!(state.service_checks.get_untracked(), vec!["download"]);
    assert_eq!(
        state.service_report.get_untracked(),
        "download: Android device required"
    );
}

#[test]
fn android_demo_dialog_keeps_outside_presses_for_back_gesture() {
    let h = Harness::open();
    let state = h.window.scope.with(|| {
        let state = lab::State::new();
        provide_local_context(state);
        state
    });
    h.mount(|| view! { lab::Lab() });
    state.dialog.set(true);
    h.window.frame();
    h.click(h.find("lab"));
    assert!(
        state.dialog.get_untracked(),
        "outside press must not dismiss gesture-test dialog"
    );
    h.click(h.find("zui-dialog"));
    assert!(
        state.dialog.get_untracked(),
        "dialog body press must not dismiss it"
    );
    h.press(h.find("zui-dialog"), zgui::vocab::NamedKey::BrowserBack);
    assert!(!state.dialog.get_untracked());
}
