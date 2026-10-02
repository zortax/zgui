use std::cell::Cell;
use std::rc::Rc;

use zgui_platform::{MenuId, MenuRole, SurfaceEvent};
use zgui_vocab::{Key, Modifiers, Timestamp};

use crate::desktop::{AppHooks, replay};

#[test]
fn a_callback_runs_until_its_guard_is_dropped() {
    let hooks = AppHooks::new();
    let runs = Rc::new(Cell::new(0));
    let guard = {
        let runs = Rc::clone(&runs);
        hooks.add_reopen(move || runs.set(runs.get() + 1))
    };
    hooks.reopen();
    drop(guard);
    hooks.reopen();
    assert_eq!(runs.get(), 1);
}

#[test]
fn a_menu_callback_receives_the_id_that_was_picked() {
    let hooks = AppHooks::new();
    let picked = Rc::new(std::cell::RefCell::new(None));
    let _guard = {
        let picked = Rc::clone(&picked);
        hooks.add_menu(move |id| *picked.borrow_mut() = Some(id.clone()))
    };
    hooks.menu(&MenuId::new("file.close"));
    assert_eq!(
        picked.borrow().as_ref().map(MenuId::as_str),
        Some("file.close")
    );
}

#[test]
fn a_callback_may_register_another_while_it_runs() {
    let hooks = AppHooks::new();
    let held = Rc::new(std::cell::RefCell::new(Vec::new()));
    let _guard = {
        let hooks2 = hooks.clone();
        let held = Rc::clone(&held);
        hooks.add_exit(move || held.borrow_mut().push(hooks2.add_exit(|| {})))
    };
    hooks.exit();
    assert_eq!(held.borrow().len(), 1);
}

#[test]
fn an_editing_role_replays_as_its_shortcut() {
    let Some(SurfaceEvent::Key {
        event, modifiers, ..
    }) = replay(MenuRole::Redo, Timestamp::ORIGIN)
    else {
        panic!("redo has a shortcut");
    };
    assert_eq!(modifiers, Modifiers::PRIMARY | Modifiers::SHIFT);
    assert_eq!(event.key, Key::character("Z"));
    assert_eq!(event.key_without_modifiers, Key::character("z"));
    assert!(replay(MenuRole::About, Timestamp::ORIGIN).is_none());
}
