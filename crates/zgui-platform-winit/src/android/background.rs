//! Work that runs while no activity is visible: a foreground service, scheduled jobs, alarms,
//! notification actions and device restart.
//!
//! Android starts these in a process that may have no activity, so `android_main` does not
//! run there. The application defines one handler with
//! [`android_background!`](crate::android_background); `ZguiService`, `ZguiJobs` and
//! `ZguiReceiver` call it for each [`Event`].
//!
//! ```ignore
//! zgui::app::android_background!(background);
//!
//! fn background(event: zgui::app::android::background::Event) {
//!     use zgui::app::android::background::Event;
//!     if let Event::Alarm { id } = event { /* post a notification */ }
//! }
//! ```

use super::jvm;
use super::notification::Notification;
use jni::EnvUnowned;
use jni::objects::{JClass, JObject, JObjectArray, JString};
use std::time::{Duration, SystemTime};

/// One reason Android runs the background handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// [`start_service`] or [`send_service`] delivered `action`. Runs on the service process's
    /// main thread; return quickly and keep long work on other threads.
    Service {
        /// Action given to [`start_service`].
        action: String,
        /// Payload given to [`start_service`].
        payload: String,
    },
    /// Android refused to make the service foreground, for example for a missing permission.
    ServiceFailed {
        /// Action of the refused start.
        action: String,
        /// Android's reason.
        error: String,
    },
    /// The service stopped, through [`stop_service`] or because Android ended it.
    ServiceStopped,
    /// A job from [`schedule_job`] runs. The job ends when the handler returns.
    Job {
        /// Job id.
        id: i32,
    },
    /// An alarm from [`schedule_alarm`] fired.
    Alarm {
        /// Alarm id.
        id: i32,
    },
    /// The device started. Alarms are scheduled again already; jobs that persist are too.
    Boot,
    /// The user tapped an action button of a notification.
    Action {
        /// Notification id.
        notification: i32,
        /// Action id.
        action: String,
        /// Notification payload.
        payload: String,
    },
}

impl Event {
    fn parse(kind: &str, args: Vec<String>) -> Option<Self> {
        let mut args = args.into_iter();
        let mut next = || args.next().unwrap_or_default();
        Some(match kind {
            "service" => Self::Service {
                action: next(),
                payload: next(),
            },
            "service.failed" => Self::ServiceFailed {
                action: next(),
                error: next(),
            },
            "service.stopped" => Self::ServiceStopped,
            "job" => Self::Job {
                id: next().parse().ok()?,
            },
            "alarm" => Self::Alarm {
                id: next().parse().ok()?,
            },
            "boot" => Self::Boot,
            "action" => Self::Action {
                notification: next().parse().ok()?,
                action: next(),
                payload: next(),
            },
            _ => return None,
        })
    }
}

/// Defines the application's background handler, a `fn(Event)`.
///
/// Expands to the native method `ZguiService`, `ZguiJobs` and `ZguiReceiver` call. Use it once
/// per application, in the library crate Android loads.
#[macro_export]
macro_rules! android_background {
    ($handler:path) => {
        #[allow(non_snake_case, unsafe_code)]
        #[unsafe(no_mangle)]
        extern "system" fn Java_dev_zgui_android_Zgui_background<'a>(
            env: $crate::android::__private::EnvUnowned<'a>,
            class: $crate::android::__private::JClass<'a>,
            context: $crate::android::__private::JObject<'a>,
            kind: $crate::android::__private::JString<'a>,
            args: $crate::android::__private::JObjectArray<'a>,
        ) {
            $crate::android::__private::dispatch(env, class, context, kind, args, $handler);
        }
    };
}

/// Runs `handler` for one background event. Called by [`android_background!`] code.
#[doc(hidden)]
pub fn dispatch(
    mut env: EnvUnowned<'_>,
    class: JClass<'_>,
    context: JObject<'_>,
    kind: JString<'_>,
    args: JObjectArray<'_>,
    handler: fn(Event),
) {
    env.with_env(|env| -> jni::errors::Result<()> {
        jvm::install(env, &class, &context)?;
        let kind = kind.to_string();
        let args = jvm::read_args(env, &args)?;
        if let Some(event) = Event::parse(&kind, args) {
            handler(event);
        }
        Ok(())
    })
    .resolve::<jni::errors::LogErrorAndDefault>();
}

/// Starts `ZguiService` in the foreground with `notification`, then delivers
/// [`Event::Service`] with `action` and `payload` in the service's process.
///
/// Android 12 and newer allow this only while the application is visible, or from some
/// background events such as an exact alarm. The manifest's `foregroundServiceType` decides
/// what the service may do; a `location` service needs location permission before it starts.
/// Post the same notification id again with [`notification::post`](super::notification::post)
/// to change its text.
///
/// # Errors
///
/// Android refused the start.
pub fn start_service(
    action: &str,
    payload: &str,
    notification: &Notification,
) -> Result<(), String> {
    let mut args = vec![action.to_owned(), payload.to_owned(), "1".to_owned()];
    args.extend(notification.args());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    jvm::call("service", 0, &args).map(drop)
}

/// Delivers [`Event::Service`] to `ZguiService` without making it foreground. A running
/// foreground service stays foreground. Works only while the application is visible or the
/// service already runs.
///
/// # Errors
///
/// Android refused the start.
pub fn send_service(action: &str, payload: &str) -> Result<(), String> {
    jvm::call("service", 0, &[action, payload, "0"]).map(drop)
}

/// Stops `ZguiService`, which then delivers [`Event::ServiceStopped`].
///
/// # Errors
///
/// The JNI call failed.
pub fn stop_service() -> Result<(), String> {
    jvm::call("service.stop", 0, &[]).map(drop)
}

/// Delivers [`Event::Alarm`] at `at`, also after the process ends or the device restarts.
///
/// The alarm is exact when the user allows exact alarms (Android 12+ settings, see
/// [`system::Settings::Alarms`](super::system::Settings::Alarms)); otherwise Android may delay
/// it. A new alarm with the same id replaces the old one.
///
/// # Errors
///
/// The JNI call failed.
pub fn schedule_alarm(id: i32, at: SystemTime) -> Result<(), String> {
    let ms = at
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    jvm::call("alarm", 0, &[&id.to_string(), &ms.to_string()]).map(drop)
}

/// Cancels the alarm `id`.
///
/// # Errors
///
/// The JNI call failed.
pub fn cancel_alarm(id: i32) -> Result<(), String> {
    jvm::call("alarm.cancel", 0, &[&id.to_string()]).map(drop)
}

/// Network a job needs before it runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JobNetwork {
    /// No network.
    #[default]
    None,
    /// Any connected network.
    Any,
    /// A network without data limits, such as most Wi-Fi.
    Unmetered,
}

/// Work for Android's job scheduler, which runs it when the conditions hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Job {
    /// Job id. A new job with the same id replaces the old one.
    pub id: i32,
    /// Earliest start after scheduling.
    pub delay: Duration,
    /// Latest start after scheduling, whatever the conditions.
    pub deadline: Option<Duration>,
    /// Network the job needs.
    pub network: JobNetwork,
    /// Run only while charging.
    pub charging: bool,
    /// Keep the job after the device restarts. Needs `RECEIVE_BOOT_COMPLETED`.
    pub persisted: bool,
}

/// Schedules `job`; Android delivers [`Event::Job`] when it runs.
///
/// # Errors
///
/// The scheduler refused the job, for example `persisted` without `RECEIVE_BOOT_COMPLETED`.
pub fn schedule_job(job: &Job) -> Result<(), String> {
    let network = match job.network {
        JobNetwork::None => "0",
        JobNetwork::Any => "1",
        JobNetwork::Unmetered => "2",
    };
    let deadline = job
        .deadline
        .map_or(-1, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    jvm::call(
        "job",
        0,
        &[
            &job.id.to_string(),
            &job.delay.as_millis().to_string(),
            &deadline.to_string(),
            network,
            if job.charging { "1" } else { "0" },
            if job.persisted { "1" } else { "0" },
        ],
    )
    .map(drop)
}

/// Cancels the job `id`.
///
/// # Errors
///
/// The JNI call failed.
pub fn cancel_job(id: i32) -> Result<(), String> {
    jvm::call("job.cancel", 0, &[&id.to_string()]).map(drop)
}
