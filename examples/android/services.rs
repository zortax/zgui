//! Android services through `zgui::app::android`. Desktop keeps the controls visible.
#[path = "service_action.rs"]
mod service_action;
use service_action::ServiceActionProps;
use zgui::prelude::*;
use zgui_ui::prelude::*;

#[cfg(target_os = "android")]
mod native {
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::sync::{LazyLock, Mutex};
    use std::time::{Duration, SystemTime};
    use zgui::app::android::{
        Guard, background, bluetooth, document, intent, location, media, nfc, notification,
        permission, power, security, system, usb,
    };
    use zgui::prelude::*;

    type Report = Box<dyn Fn(String) + Send>;
    static REPORT: Mutex<Option<Report>> = Mutex::new(None);
    static OUTCOME: Mutex<String> = Mutex::new(String::new());
    /// Ongoing operations by name; replacing or removing one stops it.
    static GUARDS: LazyLock<Mutex<HashMap<&'static str, Guard>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    static LAST_BLE: Mutex<Option<String>> = Mutex::new(None);

    fn say(text: impl Into<String>) {
        let text = text.into();
        *OUTCOME.lock().expect("outcome lock") = text.clone();
        if let Some(report) = REPORT.lock().expect("report lock").as_ref() {
            report(text);
        }
    }

    fn keep(name: &'static str, guard: Guard) {
        GUARDS.lock().expect("guard lock").insert(name, guard);
    }

    fn stop(name: &'static str) {
        let guard = GUARDS.lock().expect("guard lock").remove(name);
        drop(guard);
    }

    /// Stops `name` after `after`, unless it ended already.
    fn stop_later(name: &'static str, after: Duration, message: &'static str) {
        std::thread::spawn(move || {
            std::thread::sleep(after);
            let guard = GUARDS.lock().expect("guard lock").remove(name);
            if guard.is_some() {
                drop(guard);
                say(message);
            }
        });
    }

    /// Results of background work, which may run while this process does not exist.
    fn results_file() -> Option<std::path::PathBuf> {
        system::dirs()
            .ok()
            .map(|(files, _)| files.join("services.txt"))
    }

    fn record(key: &str, value: &str) {
        let Some(path) = results_file() else { return };
        let old = std::fs::read_to_string(&path).unwrap_or_default();
        let prefix = format!("{key}\t");
        let mut lines: Vec<String> = old
            .lines()
            .filter(|line| !line.starts_with(&prefix))
            .map(str::to_owned)
            .collect();
        lines.push(format!("{prefix}{}", value.replace('\n', " ")));
        let _ = std::fs::write(path, lines.join("\n"));
    }

    fn recorded(key: &str) -> String {
        let prefix = format!("{key}\t");
        results_file()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| {
                text.lines()
                    .find_map(|line| line.strip_prefix(&prefix).map(str::to_owned))
            })
            .unwrap_or_else(|| "none".into())
    }

    fn now() -> u128 {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    }

    /// Android calls this for services, jobs, alarms, restart and notification actions,
    /// also in a process where the activity never started.
    pub(crate) fn background(event: background::Event) {
        use background::Event;
        match event {
            Event::Service { .. } => {
                record("foreground", &format!("started {}", now()));
                std::thread::spawn(|| {
                    for tick in 1..=30 {
                        std::thread::sleep(Duration::from_secs(1));
                        record("foreground", &format!("tick {tick} at {}", now()));
                    }
                    let _ = background::stop_service();
                });
            }
            Event::ServiceFailed { error, .. } => {
                record("foreground", &format!("refused: {error}"));
            }
            Event::ServiceStopped => record("foreground", &format!("stopped {}", now())),
            Event::Job { id } => record("job", &format!("job {id} completed {}", now())),
            Event::Alarm { id } => {
                record("alarm", &format!("alarm {id} delivered {}", now()));
                let _ = notification::post(&notification::Notification {
                    id: 102,
                    title: "Durable alarm".into(),
                    text: "Alarm delivered after activity/process exit".into(),
                    payload: "alarm".into(),
                    ..Default::default()
                });
            }
            Event::Boot => record("boot", &format!("received {}", now())),
            Event::Action {
                notification: id,
                action,
                ..
            } => {
                record("intent", &format!("Notification action {action}"));
                if action == "stop" {
                    let _ = background::stop_service();
                } else {
                    let _ = notification::cancel(id);
                }
            }
        }
    }

    pub(crate) fn init() {
        intent::on_intent(|intent| {
            let mut text = format!(
                "Intent: {} {}",
                intent.action.as_deref().unwrap_or("none"),
                intent.data.as_deref().unwrap_or("")
            );
            if let Some(shared) = &intent.text {
                text += &format!("\nShared text: {shared}");
            }
            if let Some(payload) = &intent.payload {
                text += &format!("\nNotification / widget / shortcut: {payload}");
            }
            if let Some(stream) = intent.stream {
                text += &format!("\nShared file: {stream}");
                std::thread::spawn(move || match document::reopen(&stream, false) {
                    Ok(file) => say(format!("Received shared file: {stream}\n{}", preview(file))),
                    Err(error) => say(format!("Read shared file failed: {error}")),
                });
            }
            record("intent", &text);
            say(text);
        });
        system::on_window_mode(|mode| {
            say(format!(
                "Multi-window={}; PiP={}",
                mode.multi_window, mode.picture_in_picture
            ));
        });
        system::on_gamepad(|pad| say(format!("Gamepad axes: {:.2}, {:.2}", pad.x, pad.y)));
    }

    fn preview(mut file: impl Read) -> String {
        let mut buffer = vec![0; 4096];
        let count = file.read(&mut buffer).unwrap_or(0);
        if count == 0 {
            return "Empty document".into();
        }
        String::from_utf8_lossy(&buffer[..count]).into_owned()
    }

    fn report() -> String {
        let mode = system::window_mode().unwrap_or_default();
        let devices: Vec<String> = system::input_devices()
            .into_iter()
            .map(|device| device.name)
            .collect();
        format!(
            "Last result: {}\n{}\nJob: {}\nAlarm: {}\nBoot: {}\nMulti-window={}; PiP={}\nForeground: {}\nInput devices: {}",
            OUTCOME.lock().expect("outcome lock"),
            recorded("intent"),
            recorded("job"),
            recorded("alarm"),
            recorded("boot"),
            mode.multi_window,
            mode.picture_in_picture,
            recorded("foreground"),
            devices.join(", ")
        )
    }

    /// Runs `then` once `permissions` are granted.
    fn with_permission(permissions: &'static [&'static str], then: fn()) {
        let asked = permission::request(permissions, move |granted| {
            if granted {
                then();
            } else {
                say(
                    "Permission denied or cancelled. Use App permission settings to change access; other probes remain available.",
                );
            }
        });
        if let Err(error) = asked {
            say(format!("Permission request failed: {error}"));
        }
    }

    fn network(upload: bool) {
        say(if upload {
            "upload started"
        } else {
            "download started"
        });
        std::thread::spawn(move || {
            let result = if upload {
                ureq::post("https://httpbin.org/post")
                    .header("Content-Type", "text/plain")
                    .send("ZGUI upload probe")
            } else {
                ureq::get("https://example.com/").call()
            };
            match result {
                Ok(mut response) => {
                    let status = response.status();
                    let body = response.body_mut().read_to_string().unwrap_or_default();
                    let body: String = body.chars().take(4096).collect();
                    say(format!("HTTP {status}\n{body}"));
                }
                Err(error) => say(format!("Network failed (check connectivity): {error}")),
            }
        });
    }

    fn foreground() {
        let started = background::start_service(
            "probe",
            "",
            &notification::Notification {
                id: 100,
                title: "ZGUI foreground work".into(),
                text: "Bounded 30-second work probe".into(),
                payload: "foreground".into(),
                ongoing: true,
                actions: vec![("stop".into(), "Stop".into())],
            },
        );
        say(match started {
            Ok(()) => "Foreground work started; notification has Stop action. Runs for at most 30 seconds.".into(),
            Err(error) => format!("Foreground work failed: {error}"),
        });
    }

    fn post_notification() {
        let posted = notification::post(&notification::Notification {
            id: 101,
            title: "Action notification".into(),
            text: "Tap to open, or Dismiss".into(),
            payload: "notification".into(),
            actions: vec![("dismiss".into(), "Dismiss".into())],
            ..Default::default()
        });
        say(match posted {
            Ok(()) => "Action notification posted".into(),
            Err(error) => format!("Notification failed: {error}"),
        });
    }

    fn recording_path() -> std::path::PathBuf {
        system::dirs()
            .map(|(files, _)| files.join("recording.m4a"))
            .unwrap_or_default()
    }

    fn record_audio() {
        stop("audio");
        let started = media::record_audio(&recording_path(), Duration::from_secs(3), |result| {
            stop("audio");
            say(match result {
                Ok(()) => "Microphone recording finished".into(),
                Err(error) => format!("Recording failed: {error}"),
            });
        });
        match started {
            Ok(guard) => {
                keep("audio", guard);
                say("Recording microphone for 3 seconds");
            }
            Err(error) => say(format!("Recording failed: {error}")),
        }
    }

    fn play() {
        stop("audio");
        let started = media::play_audio(&recording_path(), |end| {
            stop("audio");
            say(match end {
                media::Playback::Finished => "Playback completed; audio focus released".into(),
                media::Playback::Interrupted => "Media session paused".into(),
                media::Playback::Failed(error) => error,
            });
        });
        match started {
            Ok(guard) => {
                keep("audio", guard);
                say("Playing microphone recording; media session active");
            }
            Err(error) => say(format!("Playback failed: {error}")),
        }
    }

    fn locate() {
        let request = location::Request {
            source: location::Source::Network,
            last_known: false,
            ..Default::default()
        };
        let started = location::watch(request, |update| {
            stop("location");
            say(match update {
                location::Update::Fix(fix) => format!(
                    "Location: {:.5}, {:.5}; accuracy {:?} m",
                    fix.lat, fix.lon, fix.accuracy_m
                ),
                location::Update::Disabled(_) => "Location provider disabled".into(),
                location::Update::Failed(error) => format!("Location failed: {error}"),
            });
        });
        match started {
            Ok(guard) => {
                keep("location", guard);
                say("Waiting for approximate location (15-second limit)");
                stop_later(
                    "location",
                    Duration::from_secs(15),
                    "Location timeout; no fix",
                );
            }
            Err(error) => say(format!("Location failed: {error}")),
        }
    }

    fn scan() {
        let started = bluetooth::scan(|device| {
            *LAST_BLE.lock().expect("BLE lock") = Some(device.address.clone());
            say(format!("BLE {} RSSI={}", device.address, device.rssi));
        });
        match started {
            Ok(guard) => {
                keep("scan", guard);
                say("BLE scan running for 10 seconds; nearby addresses/RSSI appear here");
                stop_later("scan", Duration::from_secs(10), "BLE scan finished");
            }
            Err(error) => say(format!("BLE scan failed: {error}")),
        }
    }

    fn connect_ble() {
        let Some(address) = LAST_BLE.lock().expect("BLE lock").clone() else {
            say("Scan and discover BLE device first");
            return;
        };
        let started = bluetooth::connect(&address, |state| {
            say(match state {
                bluetooth::Gatt::Connected => "GATT connected; discovering services".into(),
                bluetooth::Gatt::Disconnected(status) => {
                    format!("GATT disconnected; status={status}")
                }
                bluetooth::Gatt::Services(uuids) => {
                    format!("GATT services:\n{}", uuids.join("\n"))
                }
            });
        });
        match started {
            Ok(guard) => {
                keep("gatt", guard);
                say(
                    "Connecting to last discovered BLE device; service UUIDs appear after discovery",
                );
            }
            Err(error) => say(format!("GATT failed: {error}")),
        }
    }

    fn read_nfc() {
        let started = nfc::read(|tag| {
            let id: String = tag.id.iter().map(|b| format!("{b:02x}")).collect();
            say(format!(
                "NFC tag: {id} {}\nNDEF: {} bytes",
                tag.techs.join(", "),
                tag.ndef.map_or(0, |n| n.len())
            ));
        });
        match started {
            Ok(guard) => {
                keep("nfc", guard);
                say("NFC reader enabled while app visible; tap tag");
            }
            Err(error) => say(format!("NFC failed: {error}")),
        }
    }

    fn usb_probe() {
        let Some(device) = usb::devices().into_iter().next() else {
            say("No USB host devices attached");
            return;
        };
        if device.permitted {
            say(match usb::open(&device.name) {
                Ok(_fd) => format!(
                    "USB open; interfaces={}; {}",
                    device.interfaces, device.name
                ),
                Err(error) => format!("USB open failed: {error}"),
            });
            return;
        }
        let name = device.name.clone();
        let asked = usb::request_permission(&device.name, move |granted| {
            say(format!("USB permission for {name}: granted={granted}"));
        });
        say(match asked {
            Ok(()) => format!("USB access requested: {}", device.name),
            Err(error) => format!("USB request failed: {error}"),
        });
    }

    fn keystore() {
        let plain = b"ZGUI secret probe";
        let result = security::encrypt("zgui-lab", plain)
            .and_then(|sealed| security::decrypt("zgui-lab", &sealed))
            .and_then(|opened| {
                (opened == plain)
                    .then_some(())
                    .ok_or_else(|| "Keystore round trip mismatch".to_owned())
            });
        say(match result {
            Ok(()) => "AndroidKeyStore AES-GCM round trip passed".into(),
            Err(error) => format!("Keystore failed: {error}"),
        });
    }

    fn documents(action: &str) {
        let result = match action {
            "open" => document::open("*/*", |picked| {
                say(match picked {
                    Ok(Some(doc)) => {
                        record("document", &doc.uri);
                        format!("Document {}\n{}", doc.uri, preview(doc.file))
                    }
                    Ok(None) => "Open cancelled".into(),
                    Err(error) => format!("Open failed: {error}"),
                });
            }),
            "create" => document::create("zgui-report.txt", "text/plain", |picked| {
                say(match picked {
                    Ok(Some(mut doc)) => match doc.file.write_all(b"ZGUI document export\n") {
                        Ok(()) => format!("Document written: {}", doc.uri),
                        Err(error) => format!("Create document failed: {error}"),
                    },
                    Ok(None) => "Create cancelled".into(),
                    Err(error) => format!("Create failed: {error}"),
                });
            }),
            "read-document" => {
                let uri = recorded("document");
                if uri == "none" {
                    say("Open document first");
                    return;
                }
                document::reopen(&uri, false)
                    .map(|file| say(format!("Document {uri}\n{}", preview(file))))
            }
            _ => document::shared_dir()
                .and_then(|dir| {
                    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                    let path = dir.join("report.txt");
                    std::fs::write(&path, "ZGUI file sharing probe\n")
                        .map_err(|e| e.to_string())?;
                    Ok(path)
                })
                .and_then(|path| document::share_file(&path, "text/plain")),
        };
        if let Err(error) = result {
            say(format!("{action} failed: {error}"));
        }
    }

    fn photo() -> Result<(), String> {
        media::capture_photo(|photo| {
            say(match photo {
                Ok(Some(path)) => format!(
                    "Camera captured: {} ({} bytes)",
                    path.display(),
                    std::fs::metadata(&path).map_or(0, |m| m.len())
                ),
                Ok(None) => "Camera cancelled".into(),
                Err(error) => format!("Camera failed: {error}"),
            });
        })
    }

    fn authenticate() -> Result<(), String> {
        security::authenticate("ZGUI authentication probe", "", "Cancel", |auth| {
            say(match auth {
                security::Auth::Succeeded => "Biometric authentication succeeded".into(),
                security::Auth::Cancelled => "Authentication cancelled".into(),
                security::Auth::Failed(error) => format!("Biometric error: {error}"),
            });
        })
    }

    /// Runs one probe; `Ok` with a message, or the error of a refused request.
    fn run(action: &str) -> Result<Option<String>, String> {
        let done = |text: &str| Ok(Some(text.to_owned()));
        match action {
            "report" => Ok(Some(report())),
            "connectivity" => Ok(Some(match system::network()? {
                Some(net) => format!("Active network: {net:?}"),
                None => "Offline: no active network".into(),
            })),
            "download" | "upload" => {
                network(action == "upload");
                Ok(None)
            }
            "open" | "create" | "read-document" | "share-file" => {
                documents(action);
                Ok(None)
            }
            "camera" => photo().map(|()| None),
            "job" => {
                background::schedule_job(&background::Job {
                    id: 42,
                    delay: Duration::from_secs(15),
                    deadline: Some(Duration::from_secs(60)),
                    persisted: true,
                    ..Default::default()
                })?;
                done("Persisted JobScheduler job requested; refresh results after OS runs it.")
            }
            "alarm" => {
                background::schedule_alarm(42, SystemTime::now() + Duration::from_secs(15))?;
                done(
                    "Wakeup alarm requested in 15 s; survives process death and reschedules after boot.",
                )
            }
            "cancel-jobs" => {
                background::cancel_job(42)?;
                background::cancel_alarm(42)?;
                done("Job and alarm cancelled")
            }
            "foreground" => {
                with_permission(&[permission::NOTIFICATIONS], foreground);
                Ok(None)
            }
            "stop-foreground" => {
                background::stop_service()?;
                done("Foreground work stopped")
            }
            "notification" => {
                with_permission(&[permission::NOTIFICATIONS], post_notification);
                Ok(None)
            }
            "notification-settings" => {
                system::open_settings(system::Settings::Notifications).map(|()| None)
            }
            "battery" => Ok(Some(format!(
                "{:?}\nJob: {}\nAlarm: {}",
                power::status()?,
                recorded("job"),
                recorded("alarm")
            ))),
            "wake" => {
                keep("wake", power::wake_lock(Duration::from_secs(5))?);
                done("Partial wake lock acquired with 5-second timeout")
            }
            "record" => {
                with_permission(&[permission::RECORD_AUDIO], record_audio);
                Ok(None)
            }
            "play" => {
                play();
                Ok(None)
            }
            "stop-media" => {
                stop("audio");
                done("Media stopped, focus released")
            }
            "location" => {
                with_permission(&[permission::COARSE_LOCATION], locate);
                Ok(None)
            }
            "bluetooth" => {
                with_permission(
                    &[permission::BLUETOOTH_SCAN, permission::BLUETOOTH_CONNECT],
                    scan,
                );
                Ok(None)
            }
            "stop-bluetooth" => {
                stop("scan");
                done("BLE scan stopped")
            }
            "connect-ble" => {
                with_permission(&[permission::BLUETOOTH_CONNECT], connect_ble);
                Ok(None)
            }
            "disconnect-ble" => {
                stop("gatt");
                done("GATT disconnected")
            }
            "nfc" => {
                read_nfc();
                Ok(None)
            }
            "stop-nfc" => {
                stop("nfc");
                done("NFC reader disabled")
            }
            "usb" => {
                usb_probe();
                Ok(None)
            }
            "sensors" => {
                let names: Vec<String> = system::sensors()
                    .into_iter()
                    .map(|s| format!("{} (type {})", s.name, s.kind))
                    .collect();
                Ok(Some(format!("Sensors:\n{}", names.join("\n"))))
            }
            "biometric" => authenticate().map(|()| None),
            "keystore" => {
                keystore();
                Ok(None)
            }
            "account" => security::choose_account(|name| {
                say(format!(
                    "Account selected: {}",
                    name.as_deref().unwrap_or("no result")
                ));
            })
            .map(|()| None),
            "settings" => system::open_settings(system::Settings::App).map(|()| None),
            "shortcut" => {
                system::pin_shortcut("services", "ZGUI services", "shortcut")?;
                done("Pinned shortcut requested; confirm in launcher")
            }
            "widget" => {
                system::pin_widget()?;
                done("Widget pin requested; confirm in launcher")
            }
            "pip" => {
                system::enter_picture_in_picture(3, 2)?;
                done("Entering picture in picture")
            }
            _ => Ok(Some(format!("Unknown probe: {action}"))),
        }
    }

    pub(super) fn invoke(action: &str, status: RwSignal<String>) {
        let ui = ui();
        *REPORT.lock().expect("report lock") = Some(Box::new(move |text| {
            ui.post(move || status.set(text));
        }));
        match run(action) {
            Ok(Some(text)) => say(text),
            Ok(None) => {}
            Err(error) => say(format!("{action} failed: {error}")),
        }
    }
}
#[cfg(target_os = "android")]
pub(crate) use native::{background, init};

fn invoke(action: &'static str, status: RwSignal<String>) {
    #[cfg(target_os = "android")]
    native::invoke(action, status);
    #[cfg(not(target_os = "android"))]
    status.set(format!("{action}: Android device required"));
}

#[component]
pub(crate) fn Services() -> impl IntoView {
    // Application-owned signal survives changing tabs while Android finishes a request.
    let status = crate::lab::State::current().service_report;
    #[cfg(target_os = "android")]
    native::invoke("report", status);
    let groups: &[(&str, &[(&str, &str)])] = &[
        (
            "Network & files",
            &[
                ("Network / offline status", "connectivity"),
                ("HTTPS download", "download"),
                ("HTTPS upload", "upload"),
                ("Open document", "open"),
                ("Create document", "create"),
                ("Share file", "share-file"),
                ("Read retained document", "read-document"),
            ],
        ),
        (
            "Background & notifications",
            &[
                ("Schedule durable job", "job"),
                ("Schedule alarm", "alarm"),
                ("Cancel job / alarm", "cancel-jobs"),
                ("Start foreground work", "foreground"),
                ("Stop foreground work", "stop-foreground"),
                ("Action notification", "notification"),
                ("Notification settings", "notification-settings"),
                ("Battery / job results", "battery"),
                ("Hold wake lock for 5 s", "wake"),
            ],
        ),
        (
            "Camera & audio",
            &[
                ("Capture photo", "camera"),
                ("Record microphone (3 s)", "record"),
                ("Play recording", "play"),
                ("Stop media", "stop-media"),
            ],
        ),
        (
            "Device services",
            &[
                ("Request location", "location"),
                ("Bluetooth / BLE scan", "bluetooth"),
                ("Stop BLE scan", "stop-bluetooth"),
                ("Connect last BLE device", "connect-ble"),
                ("Disconnect BLE device", "disconnect-ble"),
                ("Enable NFC reader", "nfc"),
                ("Disable NFC reader", "stop-nfc"),
                ("USB devices / access", "usb"),
                ("Sensor inventory", "sensors"),
            ],
        ),
        (
            "Identity & security",
            &[
                ("Biometric authentication", "biometric"),
                ("Keystore round trip", "keystore"),
                ("Account picker", "account"),
                ("App permission settings", "settings"),
            ],
        ),
        (
            "System UI & input",
            &[
                ("Add pinned shortcut", "shortcut"),
                ("Add widget", "widget"),
                ("Enter picture in picture", "pip"),
                ("Window / input / intent report", "report"),
            ],
        ),
    ];
    view! {
        column(class = "stack") {
            column(class = "card") {
                text(class = "section-title") {"Android services"}
                text(class = "muted") {"Checkboxes record manual verification and persist across restarts. Actions run only when tapped. Permissions, missing hardware, cancellation and failures appear below. Incoming links, shares, notification actions and hardware input update this report."}
                column(class = "service-result") {
                    text(attr:data-testid = "service-report") {{move || status.get()}}
                }
                Button(variant = ButtonVariant::Outline, on:click = move |_| invoke("report", status)) {"Refresh results"}
            }
            for group in move || groups.iter().copied(), key = |group| group.0 {
                column(class = "card") {
                    text(class = "section-title") {{group.0}}
                    column(class = "stack") {
                        for item in move || group.1.iter().copied(), key = |item| item.1 {
                            ServiceAction(label = item.0, action = item.1)
                        }
                    }
                }
            }
        }
    }
}
