//! A panel down the side of a window, with a way to fold it away.
//!
//! A sidebar is a frame, a panel and a page: the frame holds whether the panel is open and what
//! shape it takes, the panel stacks bands of places to go, and the page is everything else. Every
//! part reads the frame, so folding the panel is one change that the labels, the entries, the rail
//! and the page all answer at once.

mod bands;
mod context;
mod group;
mod inset;
mod menu;
mod panel;
mod provider;
mod rail;
mod shape;
mod style;
mod trigger;

pub use crate::sidebar::bands::{
    SidebarContent, SidebarContentProps, SidebarFooter, SidebarFooterProps, SidebarHeader,
    SidebarHeaderProps, SidebarInput, SidebarInputProps, SidebarSeparator, SidebarSeparatorProps,
};
pub use crate::sidebar::context::SidebarContext;
pub use crate::sidebar::group::{
    SidebarGroup, SidebarGroupAction, SidebarGroupActionProps, SidebarGroupContent,
    SidebarGroupContentProps, SidebarGroupLabel, SidebarGroupLabelProps, SidebarGroupProps,
};
pub use crate::sidebar::inset::{SidebarInset, SidebarInsetProps};
pub use crate::sidebar::menu::{
    SidebarMenu, SidebarMenuAction, SidebarMenuActionProps, SidebarMenuBadge,
    SidebarMenuBadgeProps, SidebarMenuButton, SidebarMenuButtonProps, SidebarMenuItem,
    SidebarMenuItemProps, SidebarMenuItemState, SidebarMenuProps, SidebarMenuSkeleton,
    SidebarMenuSkeletonProps, SidebarMenuSub, SidebarMenuSubButton, SidebarMenuSubButtonProps,
    SidebarMenuSubItem, SidebarMenuSubItemProps, SidebarMenuSubProps,
};
pub use crate::sidebar::panel::{Sidebar, SidebarProps};
pub use crate::sidebar::provider::{SidebarProvider, SidebarProviderProps};
pub use crate::sidebar::rail::{SidebarRail, SidebarRailProps};
pub use crate::sidebar::shape::{
    SidebarCollapse, SidebarMenuSize, SidebarMenuVariant, SidebarSide, SidebarSubSize,
    SidebarVariant,
};
pub use crate::sidebar::trigger::{SidebarTrigger, SidebarTriggerProps};

pub use crate::sidebar::style::{SidebarBandStyle, SidebarMenuStyle, SidebarStyle};
