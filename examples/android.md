# Android Mobile Lab

Target: ARM64 Android phone, Android 8.0 / API 26 or newer; target API 35.

The demo contains no Java. Its manifest names ZGUI's prebuilt classes: `ZguiActivity` (loads the
Rust `cdylib`), `ZguiService`, `ZguiJobs`, `ZguiReceiver`, `ZguiWidget` and `ZguiFiles`. Rust drives
them through `zgui::app::android` and handles background work with `android_background!`.

Tabs:

- **Controls:** editable name with live greeting, notifications (now or delayed), checkbox,
  slider, progress, disabled button, modal with blurred backdrop.
- **Tasks:** add, complete, clear and reverse keyed rows; add 50 rows; virtualized list (only
  visible rows mount). Rows swipe to delete and long-press for a menu; pull down to sort.
- **Widgets:** chart, calendar, radio group, avatars, badges, spinner, bottom sheet, toasts,
  Arabic and Hebrew samples.
- **Visuals:** gradients, radius, shadows, clipping, CSS animation, accordion, theme switch,
  tilt (gravity sensor).
- **Phone:** multi-touch pad (drag, pinch, rotate, long-press reset), photo picker, share sheet,
  open links, worker threads.
- **Services:** files, jobs and alarms, foreground work, camera and audio, location, BLE, NFC,
  USB, biometrics and Keystore, widgets, shortcuts and PiP. Each action has a **manually
  verified** checkbox. Launching an action never sets it. Checks persist with lab state.

Lab state saves to `lab-state.txt` in app storage and returns after Android ends the process.

## Build tools

Install through SDK Manager:

- Android SDK Platform 35
- Android SDK Build-Tools 35.0.0
- Android SDK Platform-Tools
- Android NDK (Side by side) 28.2.13676358

Also install a JDK (`keytool` on `PATH`, for the debug signing key) and Python 3. Gradle and
cargo-apk are not required.

```bash
export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<installed-version>"
export PATH="$ANDROID_HOME/platform-tools:$PATH"
rustup target add aarch64-linux-android
```

`ANDROID_NDK_HOME` must name the version directory.

## Check, build, run

```bash
cargo check -p zgui-examples --example android-counter --target aarch64-linux-android
examples/android/package.sh            # or: package.sh release
adb install --no-incremental -r target/debug/apk/examples/zgui-mobile-lab.apk
adb shell am start -n dev.zgui.counter/dev.zgui.android.ZguiActivity
```

The package script links the Rust library, copies `zgui.dex` as `classes.dex`, builds resources
and manifest, aligns the APK and signs it with `target/android-toolchain/debug.keystore`. Set
`ZGUI_ANDROID_KEYSTORE` and `ZGUI_ANDROID_KEYSTORE_PASSWORD` to use a different key. Installing
over an APK signed with a different key fails. The script uses Linux NDK host tools. Use the
release APK for responsiveness checks.

A plain cargo-apk build uses stock NativeActivity and has no background or widget actions.

The example is `android-counter`, because `android` produces `libandroid.so`, which collides with
Android's system library.

Incoming link check:

```bash
adb shell am start -a android.intent.action.VIEW -d 'zgui://lab/services'
```

## System integration

- **Edge to edge.** The runtime publishes insets as root custom properties
  `--zgui-safe-area-top`, `-right`, `-bottom`, `-left` (CSS px; 0 off mobile). `zgui-ui` sheets
  and toasts pad by them.
- **Keyboard.** `--zgui-keyboard-height` holds the software keyboard height. When the keyboard
  grows, the runtime scrolls the focused element into view.
- **Text input.** A transparent native editor mirrors the focused field. Full text and selection
  snapshots update the Rust document; revisions and field identities reject stale callbacks.
  Android UTF-16 offsets convert to UTF-8 bytes. A touch focuses a field only on a stationary
  release, so a swipe that starts on a field scrolls. A hidden keyboard stays hidden until the
  user taps a field. The native editor measures with the first font given to `Fonts::register`.
- **Back.** Back closes the innermost dialog, sheet or popover, as Escape does. Unhandled Back
  moves the app to the background.
- **Theme and text size.** System dark mode reaches `prefers-color-scheme`. Font scale sets
  `--zgui-text-scale`; `zgui-ui` type tokens scale by it. `android::system_bar_theme` sets
  status bar icon colours.
- **Right to left.** An RTL system language sets `direction: rtl` on the root. `Slider` runs from
  the right edge (`NodeRef::is_rtl`).
- **Touch capture.** A pointer captured by an element (slider, drawing surface) drags that
  element and does not pan the scroller under it.
- **Refresh rate.** The host requests the fastest display mode and reports the rate in effect.
- **TalkBack.** `accesskit_android` injects into the content view: swipe navigation, reading and
  double-tap activation. Touch exploration is not verified.
- **Device services.** `zgui::app::android`: `notify`, `haptic`, `share`, `open_url`, `on_tilt`,
  `pick_image`, and modules `permission`, `intent`, `document`, `notification`, `background`,
  `location`, `bluetooth`, `nfc`, `usb`, `media`, `security`, `power`, `system`. Callbacks run
  off the UI thread; reach UI state with `ui().post`. Module docs list the manifest entry each
  needs.

## Activity lifecycle

winit allows one event loop per process, and NativeActivity's `onDestroy` waits for the native
thread to return. On Android 10+ the host reports destruction before `onDestroy`; the loop ends
and `run_android` exits the process. If Android creates an activity in a process that holds an
old loop (Android 8–9), `run_android` exits and Android starts the activity in a new process.
Handled `config_changes` (rotation, dark mode, font scale, locale, density) keep the activity
alive.

## Java classes

`crates/zgui-platform-winit/android/dev/zgui/android/*.java` compile to the committed `zgui.dex`.
Rust embeds it for stock NativeActivity applications; packaging copies it into the APK so the
manifest can name its components. After changing the Java sources, run:

```bash
crates/zgui-platform-winit/android/build-dex.sh
```

## Mali-G715

Vello 0.9 preprocessing shaders index out of bounds on Mali-G715 and lose the Vulkan device
([vello#1816](https://github.com/linebender/vello/issues/1816)). Vello CPU preprocessing uploads
about 165 MB per vector frame. ZGUI selects the coverage rasteriser on adapters named Mali-G715.
Coverage fills gradient paths flat and uses 16-sample edges; CSS gradients, shadows and backdrop
blur do not use this path.

## Device acceptance

Not proven by compilation. Record APK revision, device, OS and permission decisions.

- [ ] Cold start; reopen after removing the task.
- [ ] Notifications: permission prompt, now, delayed while in background, tap returns to app,
  Open/Dismiss actions.
- [ ] Text: type through Gboard, suggestion, swipe typing, emoji, non-ASCII; long-press select,
  handles, copy, paste; hidden keyboard stays hidden until a field is tapped.
- [ ] Back: hides keyboard, closes only the top overlay, backgrounds app at root; predictive Back
  gesture cancel and commit.
- [ ] Slider, dialog, tasks (50 rows, scroll), widgets, visuals render and respond.
- [ ] Rotate, theme, font scale, locale, split screen, PiP: state and layout survive.
- [ ] Kill process and relaunch: state and service checks restore.
- [ ] Documents: open, create, share, cancel picker, read retained URI after relaunch.
- [ ] Incoming share and `zgui://lab` link, cold and warm.
- [ ] Job and alarm delivery after process kill and reboot; cancel stops delivery.
- [ ] Foreground work stops from UI and from notification.
- [ ] Camera, microphone, location, BLE, NFC, USB: grant, deny, absent hardware.
- [ ] Biometrics success, failure, cancel; Keystore round trip; account chooser.
- [ ] Pinned widget and shortcut launch.
- [ ] TalkBack swipe, reading, double-tap, touch exploration.
- [ ] Hardware keyboard, mouse wheel, stylus, gamepad.

Not covered (needs external setup): remote push, verified HTTPS app links, OAuth.
