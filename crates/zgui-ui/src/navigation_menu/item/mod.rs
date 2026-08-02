//! One section of a navigation menu: what its parts read to find each other, and the four
//! components they are.
//!
//! | | |
//! |---|---|
//! | [`list`] | the bar the sections sit in |
//! | [`section`] | one section, and the state its trigger and its panel share |
//! | [`trigger`] | the control that opens one |
//! | [`panel`] | what it opens, portalled onto an overlay band |
//! | [`indicator`] | the arrow that points from the bar at the open panel |

mod indicator;
mod list;
mod panel;
mod section;
mod trigger;

pub use crate::navigation_menu::item::indicator::{
    NavigationMenuIndicator, NavigationMenuIndicatorProps,
};
pub use crate::navigation_menu::item::list::{NavigationMenuList, NavigationMenuListProps};
pub use crate::navigation_menu::item::panel::{NavigationMenuContent, NavigationMenuContentProps};
pub use crate::navigation_menu::item::section::{
    NavigationMenuItem, NavigationMenuItemContext, NavigationMenuItemProps,
};
pub use crate::navigation_menu::item::trigger::{
    NavigationMenuTrigger, NavigationMenuTriggerProps,
};
