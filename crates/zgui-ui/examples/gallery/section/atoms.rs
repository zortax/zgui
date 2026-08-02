//! Buttons, badges, labels, separators, skeletons, avatars, icons, and the text the atlas cannot
//! serve.

use zgui::prelude::*;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_RIGHT;
use zgui_ui_icons::set::mark::{CHECK, CROSS, PLUS};
use zgui_ui_icons::set::status::{ALERT_TRIANGLE, INFO};
use zgui_ui_icons::set::ui::{ELLIPSIS, SEARCH};

use crate::shell::{PanelProps, RowProps};

/// The atoms: every button variant and size, the badges, and the rest of the small parts.
#[component]
pub(crate) fn Atoms() -> impl IntoView {
    view! {
        Panel(title = "Button", note = "six variants, four sizes, and disabled") {
            Row(label = "variants") {
                Button {"Default"}
                Button(variant = ButtonVariant::Secondary) {"Secondary"}
                Button(variant = ButtonVariant::Destructive) {"Destructive"}
                Button(variant = ButtonVariant::Outline) {"Outline"}
                Button(variant = ButtonVariant::Ghost) {"Ghost"}
                Button(variant = ButtonVariant::Link) {"Link"}
            }
            Row(label = "sizes") {
                Button(size = ButtonSize::Xs) {"Extra small"}
                Button(size = ButtonSize::Sm) {"Small"}
                Button(size = ButtonSize::Md) {"Medium"}
                Button(size = ButtonSize::Lg) {"Large"}
            }
            Row(label = "square") {
                Button(size = ButtonSize::IconXs) {Icon(icon = PLUS, label = "Add")}
                Button(size = ButtonSize::IconSm) {Icon(icon = PLUS, label = "Add")}
                Button(size = ButtonSize::Icon) {Icon(icon = PLUS, label = "Add")}
                Button(size = ButtonSize::IconLg) {Icon(icon = PLUS, label = "Add")}
            }
            Row(label = "with a mark beside the word") {
                Button(size = ButtonSize::Sm) {Icon(icon = PLUS, label = "") "New"}
                Button {Icon(icon = PLUS, label = "") "New"}
                Button(variant = ButtonVariant::Outline) {"Next" Icon(icon = CHEVRON_RIGHT, label = "")}
                Button(disabled = true) {Spinner() "Saving"}
            }
            Row(label = "disabled") {
                Button(disabled = true) {"Default"}
                Button(variant = ButtonVariant::Outline, disabled = true) {"Outline"}
            }
        }

        Panel(title = "Badge, label, separator", note = "the labels that are not controls") {
            Row(label = "badge") {
                Badge {"Default"}
                Badge(variant = BadgeVariant::Secondary) {"Secondary"}
                Badge(variant = BadgeVariant::Destructive) {"Destructive"}
                Badge(variant = BadgeVariant::Outline) {"Outline"}
                Badge(variant = BadgeVariant::Ghost) {"Ghost"}
                Badge(variant = BadgeVariant::Link) {"Link"}
            }
            Row(label = "separator") {
                column(class = "stack wide") {
                    text {"Above"}
                    Separator()
                    text {"Below"}
                }
            }
            Row(label = "vertical") {
                row(class = "pair") {
                    text {"Left"}
                    Separator(orientation = SeparatorOrientation::Vertical)
                    text {"Right"}
                }
            }
        }

        Panel(title = "Avatar and skeleton", note = "who it is, and what has not arrived yet") {
            Row(label = "avatar") {
                Avatar(size = AvatarSize::Sm, label = "Ada Lovelace") {"AL"}
                Avatar(size = AvatarSize::Md, label = "Grace Hopper") {"GH"}
                Avatar(size = AvatarSize::Lg, label = "Barbara Liskov") {"BL"}
            }
            Row(label = "composed") {
                Avatar(label = "Ada Lovelace") {
                    AvatarFallback {"AL"}
                    AvatarBadge(label = "Online")
                }
                Avatar(size = AvatarSize::Lg, label = "Grace Hopper") {
                    AvatarFallback {"GH"}
                    AvatarBadge(label = "Verified") {Icon(icon = CHECK, label = "")}
                }
            }
            Row(label = "group") {
                AvatarGroup {
                    Avatar(label = "Ada Lovelace") {AvatarFallback {"AL"}}
                    Avatar(label = "Grace Hopper") {AvatarFallback {"GH"}}
                    Avatar(label = "Barbara Liskov") {AvatarFallback {"BL"}}
                    AvatarGroupCount {"+4"}
                }
                AvatarGroup(size = AvatarSize::Sm) {
                    Avatar(size = AvatarSize::Sm, label = "Ada Lovelace") {AvatarFallback {"AL"}}
                    Avatar(size = AvatarSize::Sm, label = "Grace Hopper") {AvatarFallback {"GH"}}
                    AvatarGroupCount {"+9"}
                }
            }
            Row(label = "skeleton") {
                column(class = "stack wide") {
                    Skeleton(style:height = "16px", style:width = "70%")
                    Skeleton(style:height = "16px")
                    Skeleton(style:height = "16px", style:width = "40%")
                }
            }
        }

        Panel(title = "Text off the atlas", note = "turned, oversized and gradient-filled runs, drawn as outlines") {
            Row(label = "rotated") {
                box(class = "turned-frame") {
                    text(class = "turned-text") {"Rotated 20\u{b0}"}
                }
            }
            Row(label = "display size") {
                text(class = "display-text") {"Ag"}
            }
            Row(label = "gradient") {
                text(class = "gradient-text") {"Gradient"}
            }
        }

        Panel(title = "Icon", note = "one path each, coloured by the inherited text colour") {
            Row(label = "marks") {
                Icon(icon = CHECK, label = "Check")
                Icon(icon = CROSS, label = "Cross")
                Icon(icon = PLUS, label = "Plus")
                Icon(icon = CHEVRON_RIGHT, label = "Next")
                Icon(icon = SEARCH, label = "Search")
                Icon(icon = ELLIPSIS, label = "More")
                Icon(icon = INFO, label = "Information")
                Icon(icon = ALERT_TRIANGLE, label = "Warning")
            }
            Row(label = "sizes") {
                Icon(icon = CHECK, size = IconSize::Sm, label = "Small")
                Icon(icon = CHECK, size = IconSize::Md, label = "Medium")
                Icon(icon = CHECK, size = IconSize::Lg, label = "Large")
            }
        }
    }
}
