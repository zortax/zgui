//! Alerts, cards, progress and the toaster.

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui::toast::Toast;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::ui::ELLIPSIS;

use crate::shell::{PanelProps, RowProps};

/// What the interface says back.
#[component]
pub(crate) fn Feedback() -> impl IntoView {
    let progress = RwSignal::new_local(Some(62.0));

    view! {
        Panel(title = "Alert", note = "a note in place, in two tones") {
            Alert {
                AlertTitle {"Heads up"}
                AlertDescription {"Components can be added to this page."}
            }
            Alert(variant = AlertVariant::Destructive) {
                AlertTitle {"Your card expires this month"}
                AlertDescription {"Update it before the next invoice."}
            }
        }

        Panel(title = "Card", note = "a header, a body and a footer") {
            Card {
                CardHeader {
                    CardTitle {"March"}
                    CardDescription {"Due on the 28th"}
                    CardAction {
                        Button(variant = ButtonVariant::Ghost, size = ButtonSize::IconSm) {
                            Icon(icon = ELLIPSIS, label = "More")
                        }
                    }
                }
                CardContent {text {"£42.00 for one seat."}}
                CardFooter {
                    Button(variant = ButtonVariant::Outline) {"Later"}
                    Button {"Pay"}
                }
            }
        }

        Panel(title = "Progress", note = "how far along, and how far along is unknown") {
            Row(label = "determinate") {
                column(class = "stack wide") {
                    Progress(value = progress, max = 100.0, label = "Upload")
                    row(class = "pair") {
                        Button(
                            size = ButtonSize::Sm,
                            variant = ButtonVariant::Secondary,
                            on:click = move |_| progress.update(|p| {
                                *p = Some((p.unwrap_or(0.0) - 10.0).max(0.0));
                            })
                        ) {
                            "Less"
                        }
                        Button(
                            size = ButtonSize::Sm,
                            variant = ButtonVariant::Secondary,
                            on:click = move |_| progress.update(|p| {
                                *p = Some((p.unwrap_or(0.0) + 10.0).min(100.0));
                            })
                        ) {
                            "More"
                        }
                        text {{move || format!("{:.0}%", progress.get().unwrap_or_default())}}
                    }
                }
            }
            Row(label = "indeterminate") {
                Progress(label = "Connecting")
            }
        }

        Panel(title = "Toast", note = "announced from anywhere below the toaster") {
            Row(label = "push one") {
                Announce()
            }
        }
    }
}

/// A pair of buttons that announce something, below the toaster that shows it.
#[component]
fn Announce() -> impl IntoView {
    let toasts = use_toaster();
    view! {
        row(class = "row__items") {
            Button(
                attr:data-testid = "toast-plain",
                on:click = move |_| {
                    if let Some(toasts) = toasts {
                        toasts.push(
                            Toast::new("Saved").description("Your changes are on the server."),
                        );
                    }
                }
            ) {
                "Save"
            }
            Button(
                variant = ButtonVariant::Destructive,
                on:click = move |_| {
                    if let Some(toasts) = toasts {
                        toasts.push(
                            Toast::new("Could not save")
                                .description("The server said no.")
                                .kind(ToastKind::Error),
                        );
                    }
                }
            ) {
                "Fail"
            }
            Button(
                variant = ButtonVariant::Secondary,
                on:click = move |_| {
                    if let Some(toasts) = toasts {
                        toasts.push(
                            Toast::new("Two people are editing this")
                                .description("Their changes will merge with yours.")
                                .kind(ToastKind::Info),
                        );
                    }
                }
            ) {
                "Inform"
            }
            Button(
                variant = ButtonVariant::Outline,
                on:click = move |_| {
                    if let Some(toasts) = toasts {
                        toasts.push(
                            Toast::new("Uploading")
                                .description("Three files to go.")
                                .kind(ToastKind::Loading),
                        );
                    }
                }
            ) {
                "Wait"
            }
        }
    }
}
