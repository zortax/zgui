//! Application state and phone-sized shell.

use crate::{controls::ControlsProps, tasks::TasksProps, visuals::VisualsProps};
use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

type Local<T> = RwSignal<T, LocalStorage>;

#[derive(Clone, Debug)]
pub(crate) struct Task {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) done: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct State {
    pub(crate) tab: Local<String>,
    pub(crate) dark: Local<bool>,
    pub(crate) enabled: Local<bool>,
    pub(crate) checked: Local<Checked>,
    pub(crate) volume: Local<f64>,
    pub(crate) name: Local<String>,
    pub(crate) draft: Local<String>,
    pub(crate) tasks: Local<Vec<Task>>,
    pub(crate) next_id: Local<u64>,
    pub(crate) motion: Local<bool>,
    pub(crate) taps: Local<u32>,
    pub(crate) dialog: Local<bool>,
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            tab: RwSignal::new_local("controls".into()),
            dark: RwSignal::new_local(true),
            enabled: RwSignal::new_local(true),
            checked: RwSignal::new_local(Checked::No),
            volume: RwSignal::new_local(35.0),
            name: RwSignal::new_local("Android explorer".into()),
            draft: RwSignal::new_local(String::new()),
            tasks: RwSignal::new_local(
                (0..6)
                    .map(|id| Task {
                        id,
                        title: [
                            "Tap switches",
                            "Drag slider",
                            "Edit your name",
                            "Open dialog",
                            "Rotate phone",
                            "Background and return",
                        ][id as usize]
                            .into(),
                        done: false,
                    })
                    .collect(),
            ),
            next_id: RwSignal::new_local(6),
            motion: RwSignal::new_local(false),
            taps: RwSignal::new_local(0),
            dialog: RwSignal::new_local(false),
        }
    }

    pub(crate) fn current() -> Self {
        use_local_context().expect("application provides lab state")
    }
}

#[component]
pub(crate) fn Lab() -> impl IntoView {
    let state = State::current();
    let scheme = Signal::derive_local(move || {
        if state.dark.get() {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        }
    });
    view! {
        ThemeProvider(scheme = scheme) {
            column(class = "lab") {
                row(class = "masthead") {
                    column(class = "heading") {
                        text(class = "eyebrow") {"ZGUI / ANDROID"}
                        text(class = "title") {"Mobile lab"}
                    }
                    spacer()
                    Button(variant = ButtonVariant::Outline, on:click = move |_| state.dark.update(|v| *v = !*v)) {
                        {move || if state.dark.get() { "Light" } else { "Dark" }}
                    }
                }
                Tabs(value = state.tab, label = "Demo sections", class = "lab-tabs") {
                    TabsList(class = "navigation") {
                        TabsTrigger(value = "controls") {"Controls"}
                        TabsTrigger(value = "tasks") {"Tasks"}
                        TabsTrigger(value = "visuals") {"Visuals"}
                    }
                    TabsContent(value = "controls", class = "panel") { Controls() }
                    TabsContent(value = "tasks", class = "panel") { Tasks() }
                    TabsContent(value = "visuals", class = "panel") { Visuals() }
                }
                text(class = "footnote") {"Native Rust • retained UI • GPU rendered"}
            }
        }
    }
}
