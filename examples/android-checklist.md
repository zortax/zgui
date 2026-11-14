# Android Mobile Lab: implementation and acceptance

Build full demo with `examples/android/package.sh`. Stock `cargo apk build` omits demo Java
components; Services actions need packaged shell. Minimum API 26; ARM64; target API 35.

Services tab has launch button and **manual verification checkbox for each action**. Checks save
with app state across restarts. Launching action never marks it verified. Record APK revision,
device/OS, permission decisions and results when copying checklist into PR.

## Implemented paths

These boxes describe code coverage. Device acceptance below remains unchecked.

- [x] UI async task, async worker, two concurrent blocking jobs, visible worker/UI thread IDs and responsiveness counter.
- [x] Android installs `zgui-tokio`; async worker awaits Tokio timer. Blocking pool remains separate from async workers.
- [x] Legacy and Android 13+ Back dispatch; overlay dismissal and root backgrounding.
- [x] Android 14+ predictive Back preview, cancellation restore and commit dispatch.
- [x] Java input dispatch in packaged shell; hover reaches AccessKit instead of being consumed by winit's native input queue. Touch, mouse wheel and keyboard forward to Rust; gamepad axes report in Services. Keyboard/gamepad activation supports arrows, Tab, Enter/D-pad center/A button.
- [x] Native IME, composition, clipboard, selection handles and keyboard avoidance.
- [x] Safe areas, dark/light system theme, text scale, RTL, rotation and surface reconstruction.
- [x] Internal file state persistence and saved manual service checks.
- [x] Connectivity status, bounded HTTPS download and fixed test-payload upload; errors visible in report.
- [x] Document open/create, retained URI read permission, bounded document preview and file sharing through grant-restricted provider.
- [x] Incoming `zgui://lab` links, shared text/files, notification/shortcut/widget payloads via activity callbacks.
- [x] JobScheduler persisted job, inexact AlarmManager alarm, boot handling, cancel controls and persisted delivery timestamps.
- [x] Bounded foreground service with visible notification, Stop action and recorded work ticks.
- [x] Notification Open/Dismiss actions, channel settings and permission denial handling.
- [x] Battery optimization/background restriction report and bounded 5-second partial wake lock.
- [x] Camera capture through external camera app and URI grant; opens captured photo through viewer.
- [x] Microphone permission, 3-second recording, playback, MediaSession, audio focus and Stop cleanup.
- [x] Approximate network location with timeout; permission/provider failures reported.
- [x] BLE discovery, explicit connection to last discovered device, GATT service discovery, disconnect and scan timeout.
- [x] NFC reader, tag technologies and cached NDEF contents; USB enumeration, permission and connection open/close.
- [x] Sensor inventory, gravity tilt and haptics.
- [x] BiometricPrompt, cancellation/errors, AndroidKeyStore AES-GCM ciphertext persistence/round trip and account chooser.
- [x] Runtime permission grant/denial/cancellation paths and app settings recovery link.
- [x] Pinned shortcut/widget requests, widget launch action, PiP and multi-window callbacks.

JobScheduler supplies durable scheduled work without adding WorkManager dependency. Alarms are
inexact; OS may defer them. These jobs survive ordinary process death, not force-stop. Force-stop
blocks background delivery until user launches app again. Foreground/media/location/scanning
probes are bounded and explicitly stopped; hardware/service availability is reported, not assumed.
NFC/USB/GATT probes inspect devices; app-specific protocols and writes require target hardware.
Account chooser selects account; it does not implement OAuth sign-in.

## External setup still required

- [ ] Remote push: needs push provider configuration and sender/backend. Local action notifications
  and OS alarm delivery are implemented; neither substitutes for server push.
- [ ] Verified HTTPS app links: needs owned domain with `assetlinks.json` and app signing fingerprint.
  Custom `zgui://lab` deep links work without domain ownership.
- [ ] OAuth/account tokens: needs app registration, provider and redirect configuration.

## Device acceptance (do not infer from compilation)

- [ ] Install signed full APK; cold start and reopen after removing task.
- [x] Run worker probe; counter stays responsive; worker IDs differ from UI; async timer completes. Pixel 9/API 37: UI 2, async 4, blocking 30/31.
- [ ] Keyboard Back hides keyboard; overlay Back closes only top layer; root Back backgrounds app.
- [ ] Back gesture progress/cancel restores view; committed gesture dispatches once.
- [ ] TalkBack swipe, reading, double-tap and touch exploration; switch-access activation.
- [ ] Finger gestures, mouse wheel, stylus, hardware keyboard shortcuts and gamepad/D-pad.
- [ ] Rotate, change theme/font scale/locale, split screen, enter/exit PiP; state/layout remain usable.
- [ ] Edit state/check service box, kill background process, relaunch; state/checks restore.
- [ ] HTTPS download/upload succeed online; offline failure returns clear result without blocking UI.
- [ ] Open/read/create/share documents, cancel picker, relaunch and read retained URI.
- [ ] Receive text/file share and `zgui://lab` link during cold start and while app runs.
- [ ] Schedule job/alarm, background app, kill process, confirm timestamps update; repeat across reboot.
- [ ] Cancel scheduled work; confirm no subsequent delivery. Observe Doze/battery restriction delays.
- [ ] Start/stop foreground work via UI and notification; timestamps update while app backgrounded.
- [ ] Grant/deny notifications; Open/Dismiss actions and channel settings work.
- [ ] Camera capture/cancel, microphone grant/deny, record/play/stop and audio-focus interruption.
- [ ] Location grant/deny/disabled provider/timeout; BLE scan/connect/discover/disconnect; NFC tag; USB grant/deny/open.
- [ ] Biometrics success/failure/cancel/no enrollment; Keystore round trip; account selection/cancel.
- [ ] Pin widget/shortcut, launch each; verify launcher unsupported case.
- [ ] Confirm permission prompts occur only after explicit action; settings recovery works after denial.
- [ ] GPU rendering, blur, gradients, clipping, animation, long-list virtualization and refresh rate on target GPU.

## Automated/build evidence

- [x] Headless Android lab tests: dialog Back, task identity, owner disposal, virtualization,
  service launch/manual-checkbox behavior and saved checkbox round trip.
- [x] Rust Android target compile and native library link.
- [x] Java host and demo classes compile; host dex rebuilt.
- [x] Full APK resource packaging, manifest/components, dex, 16 KiB ZIP alignment and signature verification.
- [x] Pixel 9/API 37 connected; APK installed and launched. See device run below for tested subset.

PR wording: “Mobile Lab now exercises Android services through packaged demo shell, including
async/Tokio work, durable jobs/alarms, input/Back/accessibility dispatch, networking/files, media,
device services, identity and system UI. Acceptance checkboxes persist separately from launch
results. Headless/build checks pass; device acceptance and externally configured push/app links
remain explicitly unverified.”

## Device run — 2026-11-12, Pixel 9, API 37

- APK replaced older stock NativeActivity demo after user approved deleting old demo data.
- Async/Tokio and two blocking workers complete on distinct threads; counter increments while work runs.
- Root Back backgrounds app; hot resume works. Dialog Back closes overlay and retains app.
- HTTPS download returns HTTP 200.
- UI automation disconnect exposed AccessKit 0.7.5 abort: Android rejects accessibility events
  while service disabled. Local vendor patch checks manager state and handles delivery exception.
- Result panel now scrolls within 180 CSS pixels; Services controls remain reachable.
- Remaining unchecked acceptance rows still need separate tests; do not mark whole checklist passed.

Device follow-up:
- [x] Dialog Back: open modal screenshot, Back, closed modal screenshot; app remains visible.
- [x] Root Back: launcher becomes resumed task; app returns through hot resume.
- [x] JobScheduler: completed timestamp stored after background/kill request.
- [x] Alarm: scheduled timestamp stored, `am kill` confirmed no app PID, receiver later stored
  `alarm=delivered 1794544481376` and removed scheduled timestamp. Reboot not tested.
- [x] Accessibility regression: UI automation tree dump/disconnect followed by tab change;
  process remains alive after manager-state guard. Full TalkBack interaction not tested.

Hardware tests, permission flows, reboot recovery, verified gesture animation, remote integrations
and remaining unchecked rows are still pending.

## Dialog/navigation regression follow-up

- Demo dialog no longer dismisses from outside presses; explicit close controls and Back remain.
- Navigation uses fixed 3-column / 2-row grid with hidden overflow; no strip scrolling.
- Tab colour transitions disabled: device log exposed paragraph brush-colour assertion during
  animated interaction. This is demo mitigation, not resolution of underlying framework bug.
- Native entry rejects same-process re-entry before Tokio initialization, avoiding repeated
  AlreadyRunning panics when Android recreates activity after native thread failure.
- [x] Seven headless tests pass, including inside/outside dialog press retention and Back dismissal.
- [x] Pixel 9: three cycles of vertical swipes over navigation plus every tab; same PID, no panic.
- [x] Pixel 9: taps inside dialog and on backdrop retain modal; injected edge swipe closes modal
  while app stays foreground. Physical gesture confirmation remains useful.
- [ ] Framework paragraph brush alias/colour-transition assertion root cause remains open.

## Touch editor activation

- Touch editor focus/caret/keyboard activation waits until stationary release within existing
  10 CSS-pixel gesture slop. Crossing threshold cancels tap even if finger returns to origin.
- Keyboard-hidden native editor no longer consumes touch gestures before Rust arbitration.
  Mouse selection and visibly active Android editor selection keep their existing paths.
- [x] Runtime text-interaction suite: 10 tests pass, including touch tap/drag/cancel regression.
- [x] Pixel 9: swipe starting in unfocused name field scrolls panel; `mInputShown=false`.
- [x] Pixel 9: tap same field opens keyboard; `mInputShown=true`.
- [x] Pixel 9: hide keyboard, drag again; `mInputShown=false`.
