//! A page of settings: the pages down one side, and what each one holds down the other.

mod context;
mod group;
mod heading;
mod item;
mod page;
mod pages;
mod pane;
mod root;
mod style;

pub use crate::settings::context::SettingsContext;
pub use crate::settings::group::{SettingsGroup, SettingsGroupProps};
pub use crate::settings::heading::{
    SettingsGroupDescription, SettingsGroupDescriptionProps, SettingsGroupLabel,
    SettingsGroupLabelProps,
};
pub use crate::settings::item::{
    SettingsItem, SettingsItemContext, SettingsItemProps, use_settings_item,
    use_settings_item_attrs,
};
pub use crate::settings::page::{SettingsPage, SettingsPageProps};
pub use crate::settings::pages::{SettingsPages, SettingsPagesProps};
pub use crate::settings::pane::{SettingsPane, SettingsPaneProps};
pub use crate::settings::root::{Settings, SettingsProps};
pub use crate::settings::style::SettingsStyle;
