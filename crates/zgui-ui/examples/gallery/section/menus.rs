//! The lists: what can be done, and what can be chosen.

use zgui::prelude::*;
use zgui::reactive::{RwSignal, UnsyncCallback};
use zgui::{component, view};
use zgui_ui::prelude::*;

use crate::shell::{PanelProps, RowProps};

/// Menus, the menu bar, and the three ways of choosing from a list.
#[component]
pub(crate) fn Menus() -> impl IntoView {
    let currency = RwSignal::new_local("gbp".to_owned());
    let country = RwSignal::new_local(String::new());
    let wrap = RwSignal::new_local(true);
    let density = RwSignal::new_local("cosy".to_owned());
    let chosen = RwSignal::new_local(String::new());

    view! {
        Panel(title = "Dropdown and context menu", note = "items, submenus, checks and radios") {
            Row(label = "dropdown") {
                DropdownMenu {
                    DropdownMenuTrigger(
                        variant = ButtonVariant::Outline,
                        attr:data-testid = "menu-trigger"
                    ) {
                        "Account"
                    }
                    DropdownMenuContent {
                        MenuLabel {"Signed in as ada"}
                        MenuSeparator()
                        MenuItem(shortcut = "⌘,") {"Settings"}
                        MenuItem {"Billing"}
                        MenuItem(disabled = true) {"Team (none yet)"}
                        MenuSeparator()
                        MenuCheckboxItem(checked = wrap) {"Wrap lines"}
                        MenuSeparator()
                        MenuRadioGroup(value = density) {
                            MenuRadioItem(value = "compact") {"Compact"}
                            MenuRadioItem(value = "cosy") {"Cosy"}
                        }
                        MenuSeparator()
                        MenuSub {
                            MenuSubTrigger {"Export"}
                            MenuSubContent {
                                MenuItem {"As CSV"}
                                MenuItem {"As JSON"}
                            }
                        }
                        MenuSeparator()
                        MenuItem(destructive = true) {"Sign out"}
                    }
                }
            }
            Row(label = "context menu") {
                ContextMenu {
                    ContextMenuTrigger {
                        box(class = "frame") {text {"Right-click here."}}
                    }
                    ContextMenuContent {
                        MenuItem {"Paste"}
                        MenuSeparator()
                        MenuItem(destructive = true) {"Clear"}
                    }
                }
            }
        }

        Panel(title = "Menubar", note = "a bar of menus, walked left and right") {
            Menubar(label = "Main menu") {
                MenubarMenu(value = "file") {
                    MenubarTrigger {"File"}
                    MenubarContent {
                        MenubarItem {"New"}
                        MenubarItem {"Open…"}
                        MenubarSeparator()
                        MenubarItem {"Quit"}
                    }
                }
                MenubarMenu(value = "edit") {
                    MenubarTrigger {"Edit"}
                    MenubarContent {
                        MenubarLabel {"History"}
                        MenubarItem {"Undo"}
                        MenubarItem {"Redo"}
                    }
                }
                MenubarMenu(value = "view") {
                    MenubarTrigger {"View"}
                    MenubarContent {
                        MenubarCheckboxItem(checked = wrap) {"Wrap lines"}
                        MenubarSeparator()
                        MenubarGroup {
                            MenubarLabel {"Density"}
                            MenubarRadioGroup(value = density) {
                                MenubarRadioItem(value = "compact") {"Compact"}
                                MenubarRadioItem(value = "cosy") {"Cosy"}
                            }
                        }
                        MenubarSeparator()
                        MenubarItem {"Full screen" MenubarShortcut {"⌃⌘F"}}
                    }
                }
            }
        }

        Panel(title = "Select and combobox", note = "chosen from a list, and searched for") {
            Row(label = "select") {
                Select(value = currency) {
                    SelectTrigger(attr:data-testid = "select-trigger", a11y:label = "Currency") {
                        SelectValue(placeholder = "Choose one")
                    }
                    SelectContent {
                        SelectGroup {
                            SelectLabel {"Europe"}
                            SelectItem(value = "gbp") {"Pound sterling"}
                            SelectItem(value = "eur") {"Euro"}
                        }
                        SelectSeparator()
                        SelectGroup {
                            SelectLabel {"Elsewhere"}
                            SelectItem(value = "usd") {"US dollar"}
                            SelectItem(value = "jpy", disabled = true) {"Japanese yen"}
                        }
                    }
                }
            }
            Row(label = "combobox") {
                Combobox(value = country) {
                    ComboboxInput(placeholder = "Search countries", label = "Country")
                    ComboboxContent {
                        ComboboxGroup {
                            ComboboxLabel {"Nearby"}
                            ComboboxItem(value = "gb") {"United Kingdom"}
                            ComboboxItem(value = "ie") {"Ireland"}
                        }
                        ComboboxSeparator()
                        ComboboxGroup {
                            ComboboxLabel {"Further"}
                            ComboboxItem(value = "fr") {"France"}
                        }
                        ComboboxEmpty {"No country by that name."}
                    }
                }
            }
        }

        Panel(title = "Command", note = "everything this program can do, in one searchable list") {
            Command {
                CommandInput(placeholder = "Type a command…", label = "Command")
                CommandList {
                    CommandGroup(label = "Invoices") {
                        CommandItem(
                            value = "invoice.new",
                            text = "New invoice",
                            on_select = UnsyncCallback::new(move |()| {
                                chosen.set("New invoice".to_owned());
                            })
                        ) {
                            "New invoice"
                        }
                        CommandItem(value = "invoice.export", text = "Export invoices") {
                            "Export invoices"
                            CommandShortcut {"⌘E"}
                        }
                    }
                    CommandSeparator()
                    CommandGroup(label = "Account") {
                        CommandItem(value = "account.settings", text = "Settings") {
                            "Settings"
                        }
                    }
                    CommandEmpty {"Nothing by that name."}
                }
            }
            text {{move || chosen.get()}}
        }
    }
}
