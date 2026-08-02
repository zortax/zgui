//! The parts a form or a list is assembled out of: items, fields, input groups, and the shapes a
//! screen takes when there is nothing on it yet.

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_RIGHT;
use zgui_ui_icons::set::mark::PLUS;
use zgui_ui_icons::set::status::INFO;
use zgui_ui_icons::set::ui::{ELLIPSIS, SEARCH};

use crate::shell::{PanelProps, RowProps};

/// Items, item groups, empties, fields, input groups, button groups, and the keyboard marks.
#[component]
pub(crate) fn Composites() -> impl IntoView {
    let search = RwSignal::new_local(String::new());
    let note = RwSignal::new_local(String::new());

    view! {
        Panel(title = "Item", note = "a row of a list: a mark, two lines, and what to do about it") {
            Row(label = "one line") {
                column(class = "stack wide") {
                    Item {
                        ItemMedia(variant = ItemMediaVariant::Icon) {Icon(icon = INFO, label = "")}
                        ItemContent {ItemTitle {"Release notes"}}
                        ItemActions {
                            Button(variant = ButtonVariant::Ghost, size = ButtonSize::IconSm) {
                                Icon(icon = CHEVRON_RIGHT, label = "Open")
                            }
                        }
                    }
                }
            }
            Row(label = "two lines") {
                column(class = "stack wide") {
                    Item(variant = ItemVariant::Outline) {
                        ItemMedia(variant = ItemMediaVariant::Icon) {Icon(icon = PLUS, label = "")}
                        ItemContent {
                            ItemTitle {"Invite a teammate"}
                            ItemDescription {"They will get an email with a link that lasts a day."}
                        }
                        ItemActions {Button(size = ButtonSize::Sm) {"Invite"}}
                    }
                }
            }
            Row(label = "group") {
                column(class = "stack wide") {
                    ItemGroup {
                        Item(size = ItemSize::Sm) {
                            ItemContent {ItemTitle {"Ada Lovelace"}}
                            ItemActions {Badge(variant = BadgeVariant::Secondary) {"Owner"}}
                        }
                        ItemSeparator()
                        Item(size = ItemSize::Sm) {
                            ItemContent {ItemTitle {"Grace Hopper"}}
                            ItemActions {Badge(variant = BadgeVariant::Outline) {"Editor"}}
                        }
                    }
                }
            }
            Row(label = "muted") {
                column(class = "stack wide") {
                    Item(variant = ItemVariant::Muted) {
                        ItemContent {ItemTitle {"Archived"}}
                    }
                }
            }
        }

        Panel(title = "Empty", note = "what a screen says when there is nothing on it") {
            Row(label = "with a media block") {
                column(class = "stack wide") {
                    Empty {
                        EmptyHeader {
                            EmptyMedia(variant = EmptyMediaVariant::Icon) {
                                Icon(icon = SEARCH, label = "")
                            }
                            EmptyTitle {"No results"}
                            EmptyDescription {"Nothing here matches that. Try a shorter word."}
                        }
                        EmptyContent {
                            Button(size = ButtonSize::Sm) {
                                "Clear the search"
                            }
                        }
                    }
                }
            }
        }

        Panel(title = "Field", note = "a label, a control, what it is for, and what is wrong") {
            Row(label = "a set of them") {
                column(class = "stack wide") {
                    FieldSet {
                        FieldLegend {"Profile"}
                        FieldGroup {
                            Field {
                                FieldLabel {"Display name"}
                                Input(default_value = "Ada", label = "Display name")
                                FieldDescription {"Shown beside everything you write."}
                            }
                            FieldSeparator {"and how to reach you"}
                            Field {
                                FieldLabel {"Email"}
                                Input(invalid = true, default_value = "ada@", label = "Email")
                                FieldError {"That address is missing its domain."}
                            }
                        }
                    }
                }
            }
            Row(label = "horizontal") {
                column(class = "stack wide") {
                    Field(orientation = FieldOrientation::Horizontal) {
                        FieldContent {
                            FieldTitle {"Weekly summary"}
                            FieldDescription {"A digest every Monday morning."}
                        }
                        Switch(a11y:label = "Weekly summary")
                    }
                }
            }
        }

        Panel(title = "Input group", note = "a field with marks, words and controls inside its frame") {
            Row(label = "leading mark") {
                column(class = "stack wide") {
                    InputGroup {
                        InputGroupAddon {Icon(icon = SEARCH, label = "")}
                        InputGroupInput(
                            value = search,
                            placeholder = "Search the library",
                            a11y:label = "Search the library"
                        )
                    }
                }
            }
            Row(label = "trailing control") {
                column(class = "stack wide") {
                    InputGroup {
                        InputGroupInput(placeholder = "Repository name", a11y:label = "Repository name")
                        InputGroupAddon(align = InputGroupAddonAlign::InlineEnd) {
                            InputGroupText {".git"}
                        }
                    }
                }
            }
            Row(label = "a textarea, and a bar under it") {
                column(class = "stack wide") {
                    InputGroup {
                        InputGroupTextarea(
                            value = note,
                            placeholder = "Leave a note",
                            a11y:label = "Leave a note"
                        )
                        InputGroupAddon(align = InputGroupAddonAlign::BlockEnd) {
                            InputGroupButton(size = InputGroupButtonSize::Xs) {"Attach"}
                            InputGroupButton(
                                size = InputGroupButtonSize::IconSm,
                                a11y:label = "Add a file"
                            ) {
                                Icon(icon = PLUS, size = {IconSize::Sm})
                            }
                            InputGroupButton(
                                variant = ButtonVariant::Default,
                                size = InputGroupButtonSize::Sm,
                            ) {"Send"}
                        }
                    }
                }
            }
        }

        Panel(title = "Button group and Kbd", note = "controls joined at the seam, and the keys that work them") {
            Row(label = "joined") {
                ButtonGroup {
                    Button(variant = ButtonVariant::Outline) {"Day"}
                    Button(variant = ButtonVariant::Outline) {"Week"}
                    Button(variant = ButtonVariant::Outline) {"Month"}
                }
            }
            Row(label = "with a menu at the end") {
                ButtonGroup {
                    Button(variant = ButtonVariant::Outline) {"Publish"}
                    Button(variant = ButtonVariant::Outline, size = ButtonSize::Icon) {
                        Icon(icon = ELLIPSIS, label = "More")
                    }
                }
            }
            // No separator here: the word's own border is the division on both of its sides, and
            // a separator next to a bordered part would draw two lines where one is meant.
            Row(label = "with a word between") {
                ButtonGroup {
                    Button(variant = ButtonVariant::Outline, size = ButtonSize::Icon) {
                        Icon(icon = PLUS, label = "Add")
                    }
                    ButtonGroupText {"12 selected"}
                    Button(variant = ButtonVariant::Outline) {"Clear"}
                }
            }
            // The separator's own place, the way shadcn's separator demo uses it: between two
            // segments that have no borders of their own to divide them.
            Row(label = "split by a separator") {
                ButtonGroup {
                    Button(variant = ButtonVariant::Secondary, size = ButtonSize::Sm) {"Copy"}
                    ButtonGroupSeparator()
                    Button(variant = ButtonVariant::Secondary, size = ButtonSize::Sm) {"Paste"}
                }
            }
            Row(label = "keys") {
                KbdGroup {Kbd {"Ctrl"} Kbd {"K"}}
                KbdGroup {Kbd {"Shift"} Kbd {"Enter"}}
            }
        }

        Panel(title = "Native select, spinner, aspect ratio", note = "the last of the small parts") {
            Row(label = "native select") {
                NativeSelect(a11y:label = "Time zone") {
                    NativeSelectOption(value = "utc") {"UTC"}
                    NativeSelectOptGroup(label = "Europe") {
                        NativeSelectOption(value = "berlin") {"Berlin"}
                        NativeSelectOption(value = "lisbon") {"Lisbon"}
                    }
                }
                NativeSelect(size = NativeSelectSize::Sm, a11y:label = "A small chooser") {
                    NativeSelectOption(value = "1") {"Small"}
                }
                NativeSelect(invalid = true, a11y:label = "A chooser that is wrong") {
                    NativeSelectOption(value = "1") {"Invalid"}
                }
            }
            Row(label = "spinner") {
                Spinner()
                Button(disabled = true) {Spinner() "Saving"}
            }
            Row(label = "aspect ratio") {
                box(class = "stack wide") {
                    AspectRatio(ratio = 16.0 / 9.0) {
                        box(class = "ratio-fill") {text {"16 : 9"}}
                    }
                }
            }
        }
    }
}
