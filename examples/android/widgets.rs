//! Charts, dates, choice groups, bottom sheet, avatars and toasts.
use crate::lab::{State, notify};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zgui::prelude::*;
use zgui_ui::prelude::*;

const WEEK: [(&str, f64); 7] = [
    ("Mon", 3.0),
    ("Tue", 5.0),
    ("Wed", 4.0),
    ("Thu", 7.0),
    ("Fri", 6.0),
    ("Sat", 2.0),
    ("Sun", 4.0),
];

#[component]
pub(crate) fn Widgets() -> impl IntoView {
    let s = State::current();
    let toasts = use_toaster();
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() / 86_400);
    let today = Date::from_days(days as i64);
    // Volume slider on Controls scales this week.
    let series = Signal::derive_local(move || {
        let scale = s.volume.get() / 35.0;
        vec![Series::new(
            "Focus hours",
            WEEK.iter()
                .map(|(day, hours)| Datum::new(*day, (hours * scale * 10.0).round() / 10.0))
                .collect(),
        )]
    });
    let rtl = RwSignal::new_local(false);
    let rtl_text = RwSignal::new_local(String::new());
    let rtl_level = RwSignal::new_local(30.0);
    let kind = move |name: &str| s.chart.with(|chosen| chosen.iter().any(|c| c == name));
    let toast = move |title: &str, kind: ToastKind| {
        if let Some(toasts) = toasts {
            toasts.push(Toast::new(title.to_owned()).kind(kind));
        }
    };
    view! {
        column(class = "stack") {
            column(class = "card") {
                row(class = "setting") {
                    column() { text(class = "section-title") {"Live chart"} text(class = "muted") {"Volume slider scales each bar"} }
                }
                ToggleGroup(value = s.chart, selection = ToggleSelection::Single, variant = ToggleVariant::Outline, label = "Chart type") {
                    ToggleGroupItem(value = "bar", label = "Bar") {"Bar"}
                    ToggleGroupItem(value = "line", label = "Line") {"Line"}
                    ToggleGroupItem(value = "area", label = "Area") {"Area"}
                }
                if move || kind("bar") { Chart(series = series, kind = ChartKind::Bar, width = 320.0, height = 190.0, legend = false, label = "Focus hours") }
                if move || kind("line") { Chart(series = series, kind = ChartKind::Line, width = 320.0, height = 190.0, legend = false, label = "Focus hours") }
                if move || kind("area") { Chart(series = series, kind = ChartKind::Area, width = 320.0, height = 190.0, legend = false, label = "Focus hours") }
            }
            column(class = "card") {
                row(class = "setting") {
                    column() { text(class = "section-title") {"Right to left"} text(class = "muted") {"Arabic and Hebrew; mirrors with an RTL system language"} }
                    spacer()
                    Switch(checked = rtl, a11y:label = "Mirror this card")
                }
                column(class = "rtl-sample", style:direction = move || Some(if rtl.get() { "rtl" } else { "ltr" }.to_owned())) {
                    text(class = "rtl-line") {"مرحبا بالعالم"}
                    text(class = "rtl-line") {"שלום עולם"}
                    text {"ZGUI يدعم 3 لغات: English, עברית, العربية"}
                    row(class = "actions") {
                        Button {"أولاً"}
                        Button(variant = ButtonVariant::Outline) {"שני"}
                        Button(variant = ButtonVariant::Outline) {"Third"}
                    }
                    text(class = "muted") {{move || format!("مستوى {:.0}٪", rtl_level.get())}}
                    Slider(value = rtl_level, step = 1.0, label = "Level", class = "touch-slider")
                    Input(value = rtl_text, label = "RTL text", placeholder = "اكتب هنا…", class = "touch-input")
                }
            }
            column(class = "card") {
                text(class = "section-title") {"Calendar"}
                text(class = "muted") {{move || match s.date.get() {
                    Some(date) => format!("Picked {date}"),
                    None => "Tap a day".to_owned(),
                }}}
                Calendar(value = s.date, today = today, label = "Pick a day")
            }
            column(class = "card") {
                text(class = "section-title") {"Delayed notification"}
                text(class = "muted") {"Used by the In … s button on Controls"}
                RadioGroup(value = s.reminder, label = "Delay", class = "choices") {
                    row(class = "choice") { RadioGroupItem(value = "5", label = "5 seconds") text {"5 seconds"} }
                    row(class = "choice") { RadioGroupItem(value = "15", label = "15 seconds") text {"15 seconds"} }
                    row(class = "choice") { RadioGroupItem(value = "30", label = "30 seconds") text {"30 seconds"} }
                }
            }
            column(class = "card") {
                text(class = "section-title") {"People & status"}
                row(class = "actions") {
                    Avatar(size = AvatarSize::Lg, label = "Ada Lovelace") {"AL"}
                    Avatar(size = AvatarSize::Lg, label = "Grace Hopper") {"GH"}
                    Avatar(size = AvatarSize::Lg, label = "Alan Turing") {
                        AvatarFallback {"AT"}
                        AvatarBadge(label = "Online")
                    }
                    spacer()
                    Spinner()
                }
                row(class = "actions") {
                    Badge {"Rust"}
                    Badge(variant = BadgeVariant::Secondary) {"wgpu"}
                    Badge(variant = BadgeVariant::Outline) {"Vulkan"}
                    Badge(variant = BadgeVariant::Destructive) {"beta"}
                }
            }
            column(class = "card") {
                text(class = "section-title") {"Sheets & toasts"}
                row(class = "actions") {
                    Drawer {
                        DrawerTrigger(variant = ButtonVariant::Outline) {"Bottom sheet"}
                        DrawerContent {
                            DrawerHeader {
                                DrawerTitle {"Share lab report"}
                                DrawerDescription {{move || format!("{} tasks, volume {:.0}%", s.tasks.with(Vec::len), s.volume.get())}}
                            }
                            DrawerFooter {
                                Button(on:click = move |_| notify(toasts, "Lab report", &format!("{} of {} tasks complete", s.tasks.with(|t| t.iter().filter(|t| t.done).count()), s.tasks.with(Vec::len)), Duration::ZERO)) {"Send as notification"}
                                DrawerClose {"Close"}
                            }
                        }
                    }
                    Button(variant = ButtonVariant::Outline, on:click = move |_| toast("Saved", ToastKind::Success)) {"Success"}
                    Button(variant = ButtonVariant::Outline, on:click = move |_| toast("Check connection", ToastKind::Warning)) {"Warning"}
                }
            }
        }
    }
}
