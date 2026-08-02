//! Checkboxes, radios, switches, toggles and sliders.

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::mark::{CHECK, CROSS, PLUS};

use crate::shell::{PanelProps, RowProps};

/// The controls that hold an answer rather than text.
#[component]
pub(crate) fn Choices() -> impl IntoView {
    let on = RwSignal::new_local(Checked::Yes);
    let off = RwSignal::new_local(Checked::No);
    let mixed = RwSignal::new_local(Checked::Mixed);
    let terms = RwSignal::new_local(Checked::No);
    let plan = RwSignal::new_local("monthly".to_owned());
    let emails = RwSignal::new_local(true);
    let bold = RwSignal::new_local(true);
    let align = RwSignal::new_local(vec!["left".to_owned()]);
    let volume = RwSignal::new_local(40.0);

    let terms_label = NodeRef::new();
    let terms_box = NodeRef::new();
    let emails_label = NodeRef::new();
    let emails_switch = NodeRef::new();

    view! {
        Panel(title = "Checkbox", note = "on, off, part-way and disabled") {
            Row(label = "states") {
                Checkbox(checked = on, a11y:label = "Checked")
                Checkbox(checked = off, a11y:label = "Unchecked")
                Checkbox(checked = mixed, a11y:label = "Mixed")
                Checkbox(checked = off, disabled = true, a11y:label = "Disabled")
            }
            Row(label = "with a label") {
                row(class = "pair") {
                    Checkbox(node_ref = terms_box, checked = terms, labelled_by = terms_label)
                    Label(node_ref = terms_label, control = terms_box) {"I accept the terms"}
                }
            }
        }

        Panel(title = "Radio and switch", note = "one of several, and one that is on or off") {
            Row(label = "radio group") {
                RadioGroup(value = plan, label = "Billing") {
                    row(class = "row__items") {
                        RadioGroupItem(value = "monthly", label = "Monthly")
                        text {"Monthly"}
                        RadioGroupItem(value = "yearly", label = "Yearly")
                        text {"Yearly"}
                        RadioGroupItem(value = "never", disabled = true, label = "Never")
                        text {"Never"}
                    }
                }
            }
            Row(label = "switch") {
                row(class = "pair") {
                    Switch(node_ref = emails_switch, checked = emails, labelled_by = emails_label)
                    Label(node_ref = emails_label, control = emails_switch) {"Send me email"}
                    Switch(checked = emails, disabled = true, a11y:label = "Disabled")
                }
            }
        }

        Panel(title = "Toggle", note = "a control that stays pressed, alone and in a group") {
            Row(label = "variants") {
                Toggle(pressed = bold, label = "Bold") {"B"}
                Toggle(variant = ToggleVariant::Outline, label = "Italic") {"I"}
                Toggle(size = ToggleSize::Sm, label = "Small") {"S"}
                Toggle(size = ToggleSize::Lg, label = "Large") {"L"}
                Toggle(disabled = true, label = "Disabled") {"D"}
            }
            Row(label = "group") {
                ToggleGroup(value = align, selection = ToggleSelection::Single, label = "Alignment") {
                    ToggleGroupItem(value = "left", label = "Left") {
                        Icon(icon = CHECK)
                    }
                    ToggleGroupItem(value = "centre", label = "Centre") {
                        Icon(icon = PLUS)
                    }
                    ToggleGroupItem(value = "right", label = "Right") {
                        Icon(icon = CROSS)
                    }
                }
            }
        }

        Panel(title = "Slider", note = "dragged, or moved with the arrows") {
            Row(label = "value") {
                column(class = "stack wide") {
                    Slider(value = volume, min = 0.0, max = 100.0, step = 5.0, label = "Volume")
                    text {{move || format!("{:.0}", volume.get())}}
                }
            }
            Row(label = "disabled") {
                Slider(value = volume, min = 0.0, max = 100.0, label = "Locked", disabled = true)
            }
        }
    }
}
