//! Accordions, collapsibles and tabs: the things that fold away.

use zgui::prelude::*;
use zgui::reactive::RwSignal;
use zgui::{component, view};
use zgui_ui::prelude::*;

use crate::shell::PanelProps;

/// What is shown one part at a time.
#[component]
pub(crate) fn Disclosure() -> impl IntoView {
    let showing = RwSignal::new_local(false);

    view! {
        Panel(title = "Accordion", note = "one answer open at a time, walked with the arrows") {
            Accordion(default_value = vec!["shipping".to_owned()]) {
                AccordionItem(value = "shipping") {
                    AccordionTrigger {"When does it ship?"}
                    AccordionContent {
                        text {"Within two working days, and sooner on a weekday."}
                    }
                }
                AccordionItem(value = "returns") {
                    AccordionTrigger {"Can I send it back?"}
                    AccordionContent {
                        text {"For thirty days, unopened."}
                    }
                }
                AccordionItem(value = "support") {
                    AccordionTrigger {"Who do I ask?"}
                    AccordionContent {text {"Anyone on the team."}}
                }
            }
        }

        Panel(title = "Collapsible", note = "one thing, folded away until it is asked for") {
            Collapsible(open = showing) {
                CollapsibleTrigger {"Delivery details"}
                CollapsibleContent {
                    column(class = "stack frame") {
                        text {"Arrives Thursday, signed for."}
                        text {"Tracked from the depot."}
                    }
                }
            }
        }

        Panel(title = "Tabs", note = "automatic activation, with one tab that cannot be reached") {
            Tabs(default_value = "profile", label = "Account") {
                TabsList {
                    TabsTrigger(value = "profile") {"Profile"}
                    TabsTrigger(value = "billing") {"Billing"}
                    TabsTrigger(value = "team", disabled = true) {"Team"}
                }
                TabsContent(value = "profile") {
                    text {"Your name, your picture and how to reach you."}
                }
                TabsContent(value = "billing") {
                    text {"Cards, invoices and the plan you are on."}
                }
                TabsContent(value = "team") {text {"Nobody yet."}}
            }
        }

        Panel(title = "Tabs, underlined", note = "the same strip with a rule under the live tab") {
            Tabs(default_value = "overview", label = "Report") {
                TabsList(variant = TabsListVariant::Line) {
                    TabsTrigger(value = "overview") {"Overview"}
                    TabsTrigger(value = "traffic") {"Traffic"}
                    TabsTrigger(value = "revenue") {"Revenue"}
                }
                TabsContent(value = "overview") {text {"Everything at once."}}
                TabsContent(value = "traffic") {text {"Where they came from."}}
                TabsContent(value = "revenue") {text {"What they paid."}}
            }
        }
    }
}
