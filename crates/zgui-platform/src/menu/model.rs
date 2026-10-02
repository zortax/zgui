//! The menu tree an application hands to its desktop.

use zgui_vocab::SharedString;

use crate::menu::role::MenuRole;
use crate::menu::shortcut::Shortcut;

/// The name of one menu action, chosen by the application.
///
/// The desktop gives the name back when the user picks the action.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MenuId(pub SharedString);

impl MenuId {
    /// An action named `id`.
    pub fn new(id: impl Into<SharedString>) -> Self {
        Self(id.into())
    }

    /// The name as text.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// One action the user can pick.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MenuAction {
    /// What the desktop gives back when the user picks it.
    pub id: MenuId,
    /// What the menu shows.
    pub label: SharedString,
    /// The chord that picks it from the keyboard.
    pub shortcut: Option<Shortcut>,
    /// Whether it can be picked now.
    pub enabled: bool,
    /// Whether it shows a check mark.
    pub checked: Option<bool>,
}

impl MenuAction {
    /// An enabled action with no shortcut and no check mark.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: MenuId::new(id),
            label: label.into(),
            shortcut: None,
            enabled: true,
            checked: None,
        }
    }

    /// The same action, picked by `shortcut` as well.
    #[must_use]
    pub fn with_shortcut(mut self, shortcut: impl Into<Option<Shortcut>>) -> Self {
        self.shortcut = shortcut.into();
        self
    }

    /// The same action, enabled or disabled.
    #[must_use]
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The same action, with a check mark that is on or off.
    #[must_use]
    pub fn with_checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }
}

/// One line of a menu.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum MenuEntry {
    /// An action of the application's own.
    Action(MenuAction),
    /// An action the framework or the desktop carries out.
    Role(MenuRole),
    /// A menu inside the menu.
    Submenu(Submenu),
    /// A line between groups.
    Separator,
}

impl From<MenuAction> for MenuEntry {
    fn from(action: MenuAction) -> Self {
        Self::Action(action)
    }
}

impl From<MenuRole> for MenuEntry {
    fn from(role: MenuRole) -> Self {
        Self::Role(role)
    }
}

impl From<Submenu> for MenuEntry {
    fn from(submenu: Submenu) -> Self {
        Self::Submenu(submenu)
    }
}

/// A titled list of entries.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Submenu {
    /// The title.
    pub label: SharedString,
    /// The entries, top to bottom.
    pub entries: Vec<MenuEntry>,
}

impl Submenu {
    /// An empty menu titled `label`.
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            entries: Vec::new(),
        }
    }

    /// The same menu with `entry` added at the bottom.
    #[must_use]
    pub fn with(mut self, entry: impl Into<MenuEntry>) -> Self {
        self.entries.push(entry.into());
        self
    }

    /// The same menu with a separator added at the bottom.
    #[must_use]
    pub fn separator(mut self) -> Self {
        self.entries.push(MenuEntry::Separator);
        self
    }

    /// Whether this menu holds `role`, at any depth.
    pub fn holds(&self, role: MenuRole) -> bool {
        self.entries.iter().any(|entry| match entry {
            MenuEntry::Role(own) => *own == role,
            MenuEntry::Submenu(submenu) => submenu.holds(role),
            _ => false,
        })
    }
}

/// The whole application menu.
///
/// On macOS the first submenu is the application menu, which the desktop titles with the
/// application's name.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct AppMenu {
    /// The menus, left to right.
    pub menus: Vec<Submenu>,
}

impl AppMenu {
    /// A menu bar with no menus.
    pub fn new() -> Self {
        Self::default()
    }

    /// The same menu bar with `submenu` added on the right.
    #[must_use]
    pub fn with(mut self, submenu: Submenu) -> Self {
        self.menus.push(submenu);
        self
    }

    /// The action named `id`, at any depth.
    pub fn action(&self, id: &str) -> Option<&MenuAction> {
        fn find<'a>(entries: &'a [MenuEntry], id: &str) -> Option<&'a MenuAction> {
            entries.iter().find_map(|entry| match entry {
                MenuEntry::Action(action) if action.id.as_str() == id => Some(action),
                MenuEntry::Submenu(submenu) => find(&submenu.entries, id),
                _ => None,
            })
        }
        self.menus
            .iter()
            .find_map(|submenu| find(&submenu.entries, id))
    }

    /// Every shortcut the menu bar claims.
    pub fn shortcuts(&self) -> Vec<Shortcut> {
        fn collect(entries: &[MenuEntry], into: &mut Vec<Shortcut>) {
            for entry in entries {
                match entry {
                    MenuEntry::Action(action) => into.extend(action.shortcut.clone()),
                    MenuEntry::Role(role) => into.extend(role.shortcut()),
                    MenuEntry::Submenu(submenu) => collect(&submenu.entries, into),
                    MenuEntry::Separator => {}
                }
            }
        }
        let mut shortcuts = Vec::new();
        for submenu in &self.menus {
            collect(&submenu.entries, &mut shortcuts);
        }
        shortcuts
    }
}
