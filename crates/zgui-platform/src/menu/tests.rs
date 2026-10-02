use zgui_vocab::{Key, Modifiers};

use crate::menu::{AppMenu, MenuAction, MenuRole, Shortcut, Submenu};

fn sample() -> AppMenu {
    AppMenu::new()
        .with(
            Submenu::new("App").with(MenuRole::About).separator().with(
                MenuAction::new("quit", "Quit")
                    .with_shortcut(Shortcut::new(Key::character("q"), Modifiers::PRIMARY)),
            ),
        )
        .with(
            Submenu::new("Edit")
                .with(MenuRole::Copy)
                .with(Submenu::new("Find").with(MenuAction::new("find", "Find…"))),
        )
}

#[test]
fn an_action_is_found_at_any_depth() {
    let menu = sample();
    assert_eq!(menu.action("find").map(|a| a.label.as_str()), Some("Find…"));
    assert!(menu.action("missing").is_none());
}

#[test]
fn the_menu_bar_claims_the_shortcuts_of_its_actions_and_roles() {
    let shortcuts = sample().shortcuts();
    assert!(shortcuts.contains(&Shortcut::new(Key::character("q"), Modifiers::PRIMARY)));
    assert!(shortcuts.contains(&MenuRole::Copy.shortcut().expect("copy has a shortcut")));
    assert_eq!(shortcuts.len(), 2);
}

#[test]
fn a_shortcut_matches_its_letter_in_either_case_and_only_its_exact_modifiers() {
    let copy = Shortcut::new(Key::character("c"), Modifiers::PRIMARY);
    assert!(copy.matches(&Key::character("C"), Modifiers::PRIMARY));
    assert!(!copy.matches(&Key::character("c"), Modifiers::PRIMARY | Modifiers::SHIFT));
    assert!(!copy.matches(&Key::character("v"), Modifiers::PRIMARY));
}

#[test]
fn only_the_editing_roles_are_replayed_as_keys() {
    let editing: Vec<_> = MenuRole::ALL
        .into_iter()
        .filter(|r| r.is_editing())
        .collect();
    assert_eq!(
        editing,
        [
            MenuRole::Undo,
            MenuRole::Redo,
            MenuRole::Cut,
            MenuRole::Copy,
            MenuRole::Paste,
            MenuRole::SelectAll
        ]
    );
    assert!(editing.iter().all(|role| role.shortcut().is_some()));
}

#[test]
fn a_submenu_knows_the_roles_it_holds() {
    let menu = sample();
    assert!(menu.menus[0].holds(MenuRole::About));
    assert!(!menu.menus[0].holds(MenuRole::Copy));
}
