//! Application state, persistence, device helpers and phone-sized shell.

use crate::services::ServicesProps;
use crate::{
    controls::ControlsProps, phone::PhoneProps, tasks::TasksProps, visuals::VisualsProps,
    widgets::WidgetsProps,
};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect};
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

type Local<T> = RwSignal<T, LocalStorage>;

/// File the application state is saved to; unset keeps state in memory only.
pub(crate) static STORE: OnceLock<PathBuf> = OnceLock::new();

#[derive(Clone, Debug)]
pub(crate) struct Task {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) done: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct State {
    pub(crate) service_report: RwSignal<String>,
    pub(crate) service_checks: Local<Vec<String>>,
    pub(crate) tab: Local<String>,
    pub(crate) scheme: Local<ColorScheme>,
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
    pub(crate) message: Local<String>,
    pub(crate) reminder: Local<String>,
    pub(crate) chart: Local<Vec<String>>,
    pub(crate) date: Local<Option<Date>>,
    pub(crate) tilting: Local<bool>,
    /// Gravity in units of g, written from the sensor thread through `ui().post`.
    pub(crate) tilt: RwSignal<(f32, f32)>,
    /// First reading after tilt switches on, which tilt is measured from.
    pub(crate) tilt_zero: RwSignal<Option<(f32, f32)>>,
    /// Picked photo, a PNG in the application cache.
    pub(crate) photo: RwSignal<Option<String>>,
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            service_checks: RwSignal::new_local(Vec::new()),
            service_report: RwSignal::new("Ready. Tap service action to begin.".into()),
            tab: RwSignal::new_local("controls".into()),
            scheme: RwSignal::new_local(ColorScheme::System),
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
            message: RwSignal::new_local("Your checklist is waiting".into()),
            reminder: RwSignal::new_local("5".into()),
            chart: RwSignal::new_local(vec!["bar".into()]),
            date: RwSignal::new_local(None),
            tilting: RwSignal::new_local(false),
            tilt: RwSignal::new((0.0, 0.0)),
            tilt_zero: RwSignal::new(None),
            photo: RwSignal::new(None),
        }
    }

    /// State saved by an earlier run, or defaults.
    pub(crate) fn load() -> Self {
        let state = Self::new();
        if let Some(text) = STORE
            .get()
            .and_then(|path| std::fs::read_to_string(path).ok())
        {
            state.restore(&text);
        }
        state
    }

    pub(crate) fn current() -> Self {
        use_local_context().expect("application provides lab state")
    }

    /// One `key=value` line per field; tasks as `task=id<TAB>done<TAB>title`. Reads are tracked.
    fn save(&self) -> String {
        let line = |text: String| text.replace(['\n', '\r'], " ");
        let mut out = format!(
            "tab={}\nscheme={}\nenabled={}\nchecked={}\nvolume={}\nname={}\nmessage={}\n\
             reminder={}\nmotion={}\ntaps={}\nnext_id={}\nchart={}\nphoto={}\n",
            self.tab.get(),
            match self.scheme.get() {
                ColorScheme::Light => "light",
                ColorScheme::Dark => "dark",
                _ => "system",
            },
            self.enabled.get(),
            self.checked.get() == Checked::Yes,
            self.volume.get(),
            line(self.name.get()),
            line(self.message.get()),
            self.reminder.get(),
            self.motion.get(),
            self.taps.get(),
            self.next_id.get(),
            self.chart.get().join(","),
            self.photo.get().unwrap_or_default(),
        );
        self.service_checks.with(|checks| {
            for check in checks {
                out += &format!("service_check={check}\n");
            }
        });
        self.tasks.with(|tasks| {
            for task in tasks {
                out += &format!(
                    "task={}\t{}\t{}\n",
                    task.id,
                    task.done,
                    line(task.title.clone())
                );
            }
        });
        out
    }

    fn restore(&self, text: &str) {
        let mut tasks = Vec::new();
        let mut checks = Vec::new();
        for (key, value) in text.lines().filter_map(|line| line.split_once('=')) {
            match key {
                "service_check" => checks.push(value.into()),
                "tab" => self.tab.set(value.into()),
                "scheme" => self.scheme.set(match value {
                    "light" => ColorScheme::Light,
                    "dark" => ColorScheme::Dark,
                    _ => ColorScheme::System,
                }),
                "enabled" => self.enabled.set(value == "true"),
                "checked" => self.checked.set(if value == "true" {
                    Checked::Yes
                } else {
                    Checked::No
                }),
                "volume" => self.volume.set(value.parse().unwrap_or(35.0)),
                "name" => self.name.set(value.into()),
                "message" => self.message.set(value.into()),
                "reminder" => self.reminder.set(value.into()),
                "motion" => self.motion.set(value == "true"),
                "taps" => self.taps.set(value.parse().unwrap_or(0)),
                "next_id" => self.next_id.set(value.parse().unwrap_or(0)),
                "chart" => self.chart.set(value.split(',').map(Into::into).collect()),
                "photo" => self.photo.set((!value.is_empty()).then(|| value.into())),
                "task" => {
                    let mut parts = value.splitn(3, '\t');
                    if let (Some(id), Some(done), Some(title)) =
                        (parts.next(), parts.next(), parts.next())
                        && let Ok(id) = id.parse()
                    {
                        tasks.push(Task {
                            id,
                            done: done == "true",
                            title: title.into(),
                        });
                    }
                }
                _ => {}
            }
        }
        self.tasks.set(tasks);
        self.service_checks.set(checks);
    }
}

/// Haptic feedback kinds. No effect off Android.
#[cfg(target_os = "android")]
pub(crate) use zgui::app::android::Haptic;
#[cfg(not(target_os = "android"))]
#[derive(Clone, Copy)]
pub(crate) enum Haptic {
    Tick,
    LongPress,
    Confirm,
}

/// Plays haptic feedback on Android.
pub(crate) fn haptic(kind: Haptic) {
    #[cfg(target_os = "android")]
    zgui::app::android::haptic(kind);
    #[cfg(not(target_os = "android"))]
    let _ = kind;
}

/// Posts an Android system notification and confirms it with an in-app toast.
/// Other platforms show only the toast.
pub(crate) fn notify(toasts: Option<ToastQueue>, title: &str, text: &str, delay: Duration) {
    #[cfg(target_os = "android")]
    let sent = zgui::app::android::notify(title, text, delay)
        .inspect_err(|error| tracing::warn!(%error, "notification failed"))
        .is_ok();
    #[cfg(not(target_os = "android"))]
    let sent = false;
    let Some(toasts) = toasts else { return };
    let toast = if !sent {
        Toast::new(title.to_owned()).description(text.to_owned())
    } else if delay.is_zero() {
        Toast::new("Notification sent").kind(ToastKind::Success)
    } else {
        Toast::new(format!("Notification in {} s", delay.as_secs()))
            .description("Press Home now; it still arrives.")
            .kind(ToastKind::Info)
    };
    toasts.push(toast);
}

/// Saves state on every change, and ticks haptics when switches flip or volume crosses a step.
fn watch(state: State) {
    let save = RenderEffect::new(move |_| {
        let text = state.save();
        if let Some(path) = STORE.get()
            && let Err(error) = std::fs::write(path, text)
        {
            eprintln!("state not saved: {error}");
        }
    });
    let switches = RenderEffect::new(move |first: Option<()>| {
        // Subscribes to each switch.
        state.enabled.track();
        state.motion.track();
        state.checked.track();
        state.tilting.track();
        if first.is_some() {
            haptic(Haptic::Tick);
        }
    });
    let steps = RenderEffect::new(move |last: Option<i64>| {
        let step = (state.volume.get() / 10.0).floor() as i64;
        if last.is_some_and(|last| last != step) {
            haptic(Haptic::Tick);
        }
        step
    });
    // Status bar icons contrast with the theme the lab shows, not only with the system theme.
    let bars = RenderEffect::new(move |_| {
        let dark = match state.scheme.get() {
            ColorScheme::Light => Some(false),
            ColorScheme::Dark => Some(true),
            _ => None,
        };
        #[cfg(target_os = "android")]
        zgui::app::android::system_bar_theme(dark);
        #[cfg(not(target_os = "android"))]
        let _ = dark;
    });
    zgui::reactive::on_cleanup_local(move || drop((save, switches, steps, bars)));
}

#[component]
pub(crate) fn Lab() -> impl IntoView {
    let state = State::current();
    watch(state);
    let scheme = Signal::derive_local(move || state.scheme.get());
    let next = move || match state.scheme.get_untracked() {
        ColorScheme::Light => ColorScheme::Dark,
        ColorScheme::Dark => ColorScheme::System,
        _ => ColorScheme::Light,
    };
    view! {
        ThemeProvider(scheme = scheme) {
          Toaster(corner = ToastCorner::BottomRight, limit = 2) {
            column(class = "lab") {
                row(class = "masthead") {
                    column(class = "heading") {
                        text(class = "eyebrow") {"ZGUI / ANDROID"}
                        text(class = "title") {"Mobile lab"}
                    }
                    spacer()
                    Button(variant = ButtonVariant::Outline, a11y:label = "Theme", on:click = move |_| state.scheme.set(next())) {
                        {move || match state.scheme.get() {
                            ColorScheme::Light => "Light",
                            ColorScheme::Dark => "Dark",
                            _ => "Auto",
                        }}
                    }
                }
                Tabs(value = state.tab, label = "Demo sections", class = "lab-tabs") {
                    TabsList(class = "navigation") {
                        TabsTrigger(value = "services") {"Services"}
                        TabsTrigger(value = "controls") {"Controls"}
                        TabsTrigger(value = "tasks") {"Tasks"}
                        TabsTrigger(value = "phone") {"Phone"}
                        TabsTrigger(value = "widgets") {"Widgets"}
                        TabsTrigger(value = "visuals") {"Visuals"}
                    }
                    TabsContent(value = "services", class = "panel") { Services() }
                    TabsContent(value = "controls", class = "panel") { Controls() }
                    TabsContent(value = "tasks", class = "panel") { Tasks() }
                    TabsContent(value = "phone", class = "panel") { Phone() }
                    TabsContent(value = "widgets", class = "panel") { Widgets() }
                    TabsContent(value = "visuals", class = "panel") { Visuals() }
                }
                text(class = "footnote") {"Native Rust • retained UI • GPU rendered"}
            }
          }
        }
    }
}

#[cfg(test)]
mod service_persistence_tests {
    use super::*;

    #[test]
    fn service_acceptance_checks_round_trip_with_existing_lab_state() {
        use zgui::reactive::{Mounted, install};
        install().ok();
        let owner = Mounted::new();
        owner.with(|| {
            let original = State::new();
            original.name.set("Mobile verifier".into());
            original
                .service_checks
                .set(vec!["download".into(), "job".into()]);
            let restored = State::new();
            restored.restore(&original.save());
            assert_eq!(
                restored.service_checks.get_untracked(),
                vec!["download", "job"]
            );
            assert_eq!(restored.name.get_untracked(), "Mobile verifier");
            assert_eq!(restored.tasks.get_untracked().len(), 6);
        });
    }
}
