//! The application menu, rebuilt whenever what it was built from changes.

use std::cell::RefCell;
use std::rc::Rc;

use zgui_platform::{AppMenu, MenuEntry, MenuId, Shortcut};
use zgui_reactive::RenderEffect;
use zgui_vocab::{KeyEvent, Modifiers};

/// The menu the application builds, and whether the desktop has seen its last version.
pub(crate) struct MenuSlot {
    /// The effect that builds the menu, kept for as long as the application runs.
    _effect: RenderEffect<()>,
    /// The version the desktop has not been given yet.
    pending: Rc<RefCell<Option<AppMenu>>>,
}

impl MenuSlot {
    /// Builds the menu with `build` now, and again whenever a signal it read changes.
    pub(crate) fn new(build: Box<dyn Fn() -> AppMenu>) -> Self {
        let pending = Rc::new(RefCell::new(None));
        let effect = {
            let pending = Rc::clone(&pending);
            RenderEffect::new(move |_| {
                let menu = build();
                *pending.borrow_mut() = Some(menu);
            })
        };
        Self {
            _effect: effect,
            pending,
        }
    }

    /// The version the desktop has not been given yet.
    pub(crate) fn take(&self) -> Option<AppMenu> {
        self.pending.borrow_mut().take()
    }
}

/// The shortcuts of the application menu's actions, shared by every window.
///
/// A window runs the action of a chord that nothing in its document claimed.
#[derive(Clone, Default)]
pub(crate) struct MenuKeys {
    /// Each shortcut with the action it runs.
    keys: Rc<RefCell<Vec<(Shortcut, MenuId)>>>,
}

impl MenuKeys {
    /// Takes the shortcuts of the enabled actions of `menu`.
    pub(crate) fn follow(&self, menu: &AppMenu) {
        fn collect(entries: &[MenuEntry], into: &mut Vec<(Shortcut, MenuId)>) {
            for entry in entries {
                match entry {
                    MenuEntry::Action(action) if action.enabled => {
                        if let Some(shortcut) = &action.shortcut {
                            into.push((shortcut.clone(), action.id.clone()));
                        }
                    }
                    MenuEntry::Submenu(submenu) => collect(&submenu.entries, into),
                    _ => {}
                }
            }
        }
        let mut keys = Vec::new();
        for submenu in &menu.menus {
            collect(&submenu.entries, &mut keys);
        }
        *self.keys.borrow_mut() = keys;
    }

    /// The action `event` under `modifiers` is the shortcut of.
    pub(crate) fn action_for(&self, event: &KeyEvent, modifiers: Modifiers) -> Option<MenuId> {
        self.keys
            .borrow()
            .iter()
            .find(|(shortcut, _)| {
                shortcut.matches(&event.key_without_modifiers, modifiers)
                    || shortcut.matches(&event.key, modifiers)
            })
            .map(|(_, id)| id.clone())
    }
}
