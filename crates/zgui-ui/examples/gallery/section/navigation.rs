//! Getting about: the trail, the pager, the site navigation and the side panel.

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::{CHEVRON_DOWN, CHEVRON_RIGHT};

use crate::shell::{PanelProps, RowProps};

/// Everything that says where one is and how to get somewhere else.
#[component]
pub(crate) fn Navigation() -> impl IntoView {
    let page = RwSignal::new_local(4_usize);

    view! {
        Panel(title = "Breadcrumb and pagination", note = "where we are, and which page of it") {
            Row(label = "breadcrumb") {
                Breadcrumb {
                    BreadcrumbList {
                        BreadcrumbItem {
                            BreadcrumbLink(on:click = move |_| ()) {"Home"}
                        }
                        BreadcrumbSeparator()
                        BreadcrumbItem {
                            BreadcrumbLink(on:click = move |_| ()) {"Settings"}
                        }
                        BreadcrumbSeparator()
                        BreadcrumbItem {BreadcrumbEllipsis()}
                        BreadcrumbSeparator()
                        BreadcrumbItem {BreadcrumbPage {"Billing"}}
                    }
                }
            }
            Row(label = "pagination") {
                Pagination {
                    PaginationContent {
                        PaginationItem {
                            PaginationPrevious(on:click = move |_| {
                                page.update(|n| *n = n.saturating_sub(1));
                            })
                        }
                        PaginationItem {
                            PaginationLink(page = 3_usize) {"3"}
                        }
                        PaginationItem {
                            PaginationLink(page = 4_usize, current = true) {"4"}
                        }
                        PaginationItem {
                            PaginationLink(page = 5_usize) {"5"}
                        }
                        PaginationItem {PaginationEllipsis()}
                        PaginationItem {
                            PaginationNext(on:click = move |_| page.update(|n| *n += 1))
                        }
                    }
                }
            }
            Row(label = "page") {
                text {{move || format!("page {}", page.get())}}
            }
        }

        Panel(title = "Navigation menu", note = "a section that opens under the bar") {
            NavigationMenu(label = "Main") {
                NavigationMenuList {
                    NavigationMenuItem(value = "products") {
                        NavigationMenuTrigger(attr:data-testid = "nav-trigger") {
                            "Products"
                        }
                        NavigationMenuContent {
                            NavigationMenuLink {"Editor"}
                            NavigationMenuLink(active = true) {"Runtime"}
                            NavigationMenuLink {"Renderer"}
                        }
                    }
                    NavigationMenuItem(value = "company") {
                        NavigationMenuTrigger {"Company"}
                        NavigationMenuContent {
                            NavigationMenuLink {"About"}
                            NavigationMenuLink {"Careers"}
                        }
                    }
                    NavigationMenuItem(value = "pricing") {
                        NavigationMenuLink {"Pricing"}
                    }
                    NavigationMenuIndicator()
                }
            }
        }

        SidebarPanel()
    }
}

/// The sidebar demo, in a room of its own.
#[component]
fn SidebarPanel() -> impl IntoView {
    view! {
        Panel(title = "Sidebar", note = "a panel down the side, folded by Ctrl+B", wide = true) {
            // A contained room rather than the page itself: the demo has to show the panel's
            // whole anatomy — header, groups, footer, inset — and folding it, which only reads
            // when there is an edge for the panel to slide behind.
            box(class = "sidebar-frame") {
                SidebarProvider {
                    Sidebar(label = "Project") {
                        SidebarHeader {text {"Acme Studio"}}
                        SidebarContent {
                            SidebarGroup {
                                SidebarGroupLabel {"Platform"}
                                SidebarGroupContent {
                                    SidebarMenu {
                                        SidebarMenuItem {
                                            SidebarMenuButton(active = true, tooltip = "Home") {
                                                "Home"
                                            }
                                        }
                                        SidebarMenuItem {
                                            SidebarMenuButton(tooltip = "Inbox") {"Inbox"}
                                            SidebarMenuBadge {"12"}
                                        }
                                        CollapsedProjects()
                                        SidebarMenuItem {
                                            SidebarMenuButton(tooltip = "Deployments") {
                                                "Deployments"
                                            }
                                        }
                                        SidebarMenuItem {
                                            SidebarMenuButton(tooltip = "Analytics") {"Analytics"}
                                        }
                                    }
                                }
                            }
                            SidebarGroup {
                                SidebarGroupLabel {"Settings"}
                                SidebarGroupContent {
                                    // The quieter register: small entries, and one outlined so
                                    // both menu variants stay on show.
                                    SidebarMenu {
                                        SidebarMenuItem {
                                            SidebarMenuButton(
                                                size = SidebarMenuSize::Sm,
                                                tooltip = "General"
                                            ) {"General"}
                                        }
                                        SidebarMenuItem {
                                            SidebarMenuButton(
                                                size = SidebarMenuSize::Sm,
                                                tooltip = "Members"
                                            ) {"Members"}
                                        }
                                        SidebarMenuItem {
                                            SidebarMenuButton(
                                                size = SidebarMenuSize::Sm,
                                                variant = SidebarMenuVariant::Outline,
                                                tooltip = "Billing"
                                            ) {"Billing"}
                                        }
                                    }
                                }
                            }
                        }
                        SidebarFooter {
                            row(class = "pair") {
                                Avatar(size = AvatarSize::Sm, label = "Ada Lovelace") {"AL"}
                                text {"ada@example.com"}
                            }
                        }
                        SidebarRail()
                    }
                    SidebarInset {
                        SidebarTrigger(attr:data-testid = "sidebar-trigger")
                        column(class = "sidebar-inset-body") {
                            text(class = "sidebar-inset-title") {"The document"}
                            text(class = "sidebar-inset-note") {
                                "Fold the panel with the trigger above, the rail on its edge or \
                                 Ctrl+B, and this inset takes the room it gives back."
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A menu entry whose sub-menu folds: the chevron says which way it stands, and pressing the
/// entry turns it.
#[component]
fn CollapsedProjects() -> impl IntoView {
    let showing = RwSignal::new_local(true);
    view! {
        SidebarMenuItem {
            SidebarMenuButton(
                tooltip = "Projects",
                on:click = move |_| showing.update(|open| *open = !*open)
            ) {
                "Projects"
                if move || showing.get() {
                    Icon(icon = CHEVRON_DOWN, label = "")
                } else {
                    Icon(icon = CHEVRON_RIGHT, label = "")
                }
            }
            if move || showing.get() {
                SidebarMenuSub {
                    SidebarMenuSubItem {SidebarMenuSubButton(active = true) {"Website"}}
                    SidebarMenuSubItem {SidebarMenuSubButton {"Mobile app"}}
                    SidebarMenuSubItem {
                        SidebarMenuSubButton(size = SidebarSubSize::Sm) {"Design system"}
                    }
                }
            } else {}
        }
    }
}
