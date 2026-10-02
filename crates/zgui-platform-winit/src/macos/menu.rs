//! The application menu, built with muda from the contract's menu tree.

use std::str::FromStr;
use std::sync::Arc;

use muda::accelerator::{Key as MudaKey, KeyAccelerator, Modifiers as MudaModifiers};
use muda::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use zgui_platform::{
    AppEvent, AppMenu, MenuEntry, MenuId, MenuRole, Shortcut, Submenu, WakeReason, Waker,
};
use zgui_vocab::{Key, Modifiers};

/// What prefixes the muda id of an application action.
const ACTION: &str = "zgui.action:";
/// What prefixes the muda id of an editing role.
const ROLE: &str = "zgui.role:";

/// The menu bar on show, kept alive for as long as it is.
#[derive(Default)]
pub(crate) struct Menus {
    /// The muda menu that owns the AppKit one.
    current: Option<Menu>,
}

impl core::fmt::Debug for Menus {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Menus")
            .field("installed", &self.current.is_some())
            .finish()
    }
}

/// Shows `menu` as the application menu, and sends what the user picks to `waker`.
pub(crate) fn install(menus: &mut Menus, menu: &AppMenu, waker: Arc<dyn Waker>) {
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some(event) = app_event(event.id().as_ref()) {
            waker.wake(WakeReason::App(event));
        }
    }));
    let bar = Menu::new();
    for submenu in &menu.menus {
        let built = build_submenu(submenu);
        if let Err(error) = bar.append(&built) {
            tracing::warn!(target: "zgui::platform", %error, "a menu could not be added");
        }
        if submenu.holds(MenuRole::Minimize) {
            built.set_as_windows_menu_for_nsapp();
        }
        if submenu.label.as_str() == "Help" {
            built.set_as_help_menu_for_nsapp();
        }
    }
    bar.init_for_nsapp();
    menus.current = Some(bar);
    crate::macos::keys::keep_native(&native_shortcuts(menu));
}

/// The shortcuts of the desktop roles in `menu`, which AppKit answers itself.
fn native_shortcuts(menu: &AppMenu) -> Vec<Shortcut> {
    fn collect(entries: &[MenuEntry], into: &mut Vec<Shortcut>) {
        for entry in entries {
            match entry {
                MenuEntry::Role(role) if !role.is_editing() => into.extend(role.shortcut()),
                MenuEntry::Submenu(submenu) => collect(&submenu.entries, into),
                _ => {}
            }
        }
    }
    let mut shortcuts = Vec::new();
    for submenu in &menu.menus {
        collect(&submenu.entries, &mut shortcuts);
    }
    shortcuts
}

/// What a picked muda id means to the application.
fn app_event(id: &str) -> Option<AppEvent> {
    if let Some(action) = id.strip_prefix(ACTION) {
        return Some(AppEvent::Menu(MenuId::new(action.to_owned())));
    }
    let index: usize = id.strip_prefix(ROLE)?.parse().ok()?;
    MenuRole::ALL.get(index).copied().map(AppEvent::Role)
}

/// The muda id of an editing role.
fn role_id(role: MenuRole) -> String {
    let index = MenuRole::ALL
        .iter()
        .position(|own| *own == role)
        .unwrap_or_default();
    format!("{ROLE}{index}")
}

/// One submenu, with everything in it.
fn build_submenu(submenu: &Submenu) -> muda::Submenu {
    let built = muda::Submenu::new(submenu.label.as_str(), true);
    for entry in &submenu.entries {
        let item: Box<dyn IsMenuItem> = match entry {
            MenuEntry::Action(action) => {
                let id = format!("{ACTION}{}", action.id.as_str());
                let accelerator = action.shortcut.as_ref().and_then(accelerator);
                match action.checked {
                    Some(checked) => {
                        let item = CheckMenuItem::with_id(
                            id,
                            action.label.as_str(),
                            action.enabled,
                            checked,
                            None,
                        );
                        let _ = item.set_key_accelerator(accelerator);
                        Box::new(item)
                    }
                    None => {
                        let item =
                            MenuItem::with_id(id, action.label.as_str(), action.enabled, None);
                        let _ = item.set_key_accelerator(accelerator);
                        Box::new(item)
                    }
                }
            }
            MenuEntry::Role(role) => role_item(*role),
            MenuEntry::Submenu(inner) => Box::new(build_submenu(inner)),
            MenuEntry::Separator => Box::new(PredefinedMenuItem::separator()),
            _ => continue,
        };
        if let Err(error) = built.append(item.as_ref()) {
            tracing::warn!(target: "zgui::platform", %error, "a menu entry could not be added");
        }
    }
    built
}

/// The entry a role stands for.
///
/// The desktop roles are AppKit's own items. The editing roles are ordinary items the framework
/// answers, because the focused element is a framework element.
fn role_item(role: MenuRole) -> Box<dyn IsMenuItem> {
    let predefined = match role {
        MenuRole::About => PredefinedMenuItem::about(None, None),
        MenuRole::Services => PredefinedMenuItem::services(None),
        MenuRole::Hide => PredefinedMenuItem::hide(None),
        MenuRole::HideOthers => PredefinedMenuItem::hide_others(None),
        MenuRole::ShowAll => PredefinedMenuItem::show_all(None),
        MenuRole::Minimize => PredefinedMenuItem::minimize(None),
        MenuRole::Zoom => PredefinedMenuItem::maximize(Some("Zoom")),
        MenuRole::Fullscreen => PredefinedMenuItem::fullscreen(None),
        MenuRole::BringAllToFront => PredefinedMenuItem::bring_all_to_front(None),
        _ => {
            let item = MenuItem::with_id(role_id(role), role.label(), true, None);
            let _ = item.set_key_accelerator(role.shortcut().as_ref().and_then(accelerator));
            return Box::new(item);
        }
    };
    Box::new(predefined)
}

/// The muda accelerator a shortcut stands for.
fn accelerator(shortcut: &Shortcut) -> Option<KeyAccelerator> {
    let key = match &shortcut.key {
        Key::Named(named) => MudaKey::from_str(named.as_str()).ok()?,
        Key::Character(text) => MudaKey::Character(text.to_lowercase()),
        _ => return None,
    };
    Some(KeyAccelerator::new(modifiers(shortcut.modifiers), key))
}

/// The muda modifiers a set of modifiers stands for.
fn modifiers(modifiers: Modifiers) -> MudaModifiers {
    let mut own = MudaModifiers::empty();
    if modifiers.shift() {
        own |= MudaModifiers::SHIFT;
    }
    if modifiers.control() {
        own |= MudaModifiers::CONTROL;
    }
    if modifiers.alt() {
        own |= MudaModifiers::ALT;
    }
    if modifiers.meta() {
        own |= MudaModifiers::META;
    }
    own
}

#[cfg(test)]
mod tests {
    use super::{app_event, role_id};
    use zgui_platform::{AppEvent, MenuId, MenuRole};

    #[test]
    fn a_picked_id_names_the_action_or_the_role_it_was_built_from() {
        assert_eq!(
            app_event("zgui.action:file.close"),
            Some(AppEvent::Menu(MenuId::new("file.close")))
        );
        assert_eq!(
            app_event(&role_id(MenuRole::Paste)),
            Some(AppEvent::Role(MenuRole::Paste))
        );
        assert_eq!(app_event("1"), None);
    }
}
