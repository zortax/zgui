# Android Mobile Lab

Bring-up target: ARM64 Android phone, Android 8.0 / API 26 or newer.
Stock `android.app.NativeActivity` loads Rust `cdylib`; ZGUI renders shipped components with
bundled font. Application contains no Java. `zgui-platform-winit` embeds compiled `ZguiHost`
class and loads it from memory at startup. Host supplies Android InputConnection, composing text,
selection handles, edit menu, clipboard and notifications.
Focused editor stays transparent while full text/selection snapshots update Rust document.
Native revisions prevent delayed Rust snapshots from overwriting newer keyboard edits; field
identities reject callbacks from an old focus target. UTF-16 Android offsets convert to UTF-8 bytes.

- **Controls:** editable name with live greeting, system notifications (now or delayed),
  checkbox, slider, progress, tap counter, disabled button, animated modal with blurred backdrop
  and vector dismiss icon.
- **Tasks:** add/remove/complete, derived count, clear completed, reverse keyed rows,
  add 50 stress rows, finger scrolling and scrollbar; only visible rows mount. Completing
  last task posts a notification.
- **Widgets:** live bar/line/area chart scaled by volume slider, calendar, radio group for
  notification delay, avatars, badges, spinner, bottom sheet and toasts.
- **Visuals:** three tiles each show one paint feature: multi-stop gradient, large radius,
  layered shadow.
- **Phone:** multi-touch pad (one-finger drag, two-finger pinch and rotate, long-press reset),
  system photo picker, share sheet and opening links.
- **Device:** haptics on switches, volume steps, completion and long press; tilt (gravity
  sensor) moves Visuals tile; task rows swipe to delete and long-press for a menu; pulling the
  list down sorts open checks first. State saves to app storage on every change and returns after
  Android ends the process. Theme defaults to system (Auto) and follows system dark mode; text
  follows system font size.
- **Visuals:** live light/dark theme, gradients, grid, shadows, clipping, CSS transform
  animation (off initially), accordion and responsive landscape layout.

## App icon

`examples/android/logo.png` is the source of an adaptive icon in `examples/android/res/`: a
foreground layer (logo, its circle about 70 of the 72 visible dp) in `mipmap-*/` and the logo's navy
as background colour. The launcher masks both to its own shape. `Cargo.toml` metadata sets
`resources` and `icon`. Notifications
show it as the large icon; the status bar keeps a system glyph, because Android draws small icons
as a silhouette.

## Notifications

`zgui::app::android_notify(title, text, delay)` asks host to post a system notification.
Android 13 and newer show permission prompt on first use, then post. Delay runs on Android main
thread, so notification arrives while app stays in background. Tapping notification returns
to app. Manifest declares `POST_NOTIFICATIONS`. Desktop build shows in-app toast only.

Application state lives above document owners, so surface reconstruction retains values.
Task completion is plain data inside application-owned list signal; new rows do not keep
signals owned by a discarded document.

## Build tools

Install Android SDK command-line tools or Android Studio. Through SDK Manager install:

- Android SDK Platform 35
- Android SDK Build-Tools 35.0.0
- Android SDK Platform-Tools
- Android NDK (Side by side), version 28.2.13676358

Accept SDK licences. Set paths for installed SDK and NDK, for example:

```bash
export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/<installed-version>"
export PATH="$ANDROID_HOME/platform-tools:$PATH"
rustup target add aarch64-linux-android
cargo install --git https://github.com/rust-mobile/cargo-apk.git \
    --rev 887423a9150df52f6699332ba39e58ae4c24c877 cargo-apk
```

Also requires JDK (`keytool` on PATH) for debug signing key. NDK path must name version directory,
not parent `ndk` directory. Initial build uses command-line tools 22.0, SDK Platform 35,
Build-Tools 35.0.0, NDK 28.2.13676358, and OpenJDK 27.0.1.
Packager revision pinned above; crates.io release differs.

## Check, build, run

From repository root:

```bash
cargo check -p zgui-examples --example android-counter --target aarch64-linux-android
cargo apk build -p zgui-examples --example android-counter
```

Check command verifies Rust compilation; SDK/NDK required for linked APK build.
APK output path appears in build output. Debug APK uses local debug signing key.
Manifest uses stock NativeActivity. Notification permission and handled configuration changes
live in `crates/zgui-examples/Cargo.toml` metadata.

Enable USB debugging on phone, connect USB, approve host, then:

```bash
adb devices
adb install --no-incremental -r target/debug/apk/examples/zgui-counter.apk
adb shell am start -n dev.zgui.counter/android.app.NativeActivity
```

Packaging follows [cargo-apk's NativeActivity example workflow](https://github.com/rust-mobile/cargo-apk).

## Optimized phone build

Use release build when assessing responsiveness. After debug build has created local test key:

```bash
export CARGO_APK_RELEASE_KEYSTORE="${ANDROID_USER_HOME:-$HOME/.android}/debug.keystore"
export CARGO_APK_RELEASE_KEYSTORE_PASSWORD=android
cargo apk build -p zgui-examples --example android-counter --release
adb install --no-incremental -r target/release/apk/examples/zgui-counter.apk
adb shell am start -n dev.zgui.counter/android.app.NativeActivity
```

This signs optimized APK with same local development certificate, so it can replace debug APK.
Release APK: `target/release/apk/examples/zgui-counter.apk` (about 12 MB in tested build).

Android prepares vector rasteriser and overlay pipelines during startup.
New CSS animations and transitions exclude their first frame's rendering cost from their timeline;
existing motion keeps elapsed time normally. Modal blur and dismiss icon remain enabled.

Pixel 9 optimized build, dialog open after idle: first vector frame fell from about 60 ms to
10 ms, later frames from about 17 ms to 2 ms, after Mali-G715 moved to coverage rasteriser
(see below).

## Device acceptance checks

1. Notifications: tap Notify now, allow permission, confirm notification. Tap In 5 s, press
   Home, confirm notification arrives; tap it to return. Complete all tasks; confirm notification.
2. Controls: edit name, confirm greeting and character count; toggle switch and checkbox.
   Type through Gboard, tap suggestion, swipe a word; each updates live greeting. Long-press text,
   drag handles, copy word, select another word and paste. Hide keyboard; verify it stays hidden
   until field tapped again. Test emoji and non-ASCII text as well as individual keys.
3. Drag slider: numeric value and progress move together. Disabled button stays inert.
4. Open dialog: blurred backdrop, vector close icon and Done button render; dismiss works.
5. Tasks: add text, complete rows, clear done, reverse order; add 50 and scroll by finger.
6. Widgets: switch chart type, drag volume and watch bars, pick day, open bottom sheet, show toasts.
7. Visuals: expand accordion, enable motion, change theme; animation moves without input.
8. Rotate, Home, return, switch tabs: names, row count, completion, slider and theme survive.
9. Disable motion and leave editable fields unfocused before measuring idle work.

Demo saves state to `lab-state.txt` in app storage, so it survives process death.
Android system text clipboard and native field selection/edit menu are connected. Native editor
measures with first font registered through `Fonts::register` (system font if none); several
fonts, transforms and selection of ordinary labels need further integration.

## System integration

- **Edge to edge.** App draws behind status bar, navigation bar and cutout. Runtime publishes
  insets as root custom properties `--zgui-safe-area-top`, `-right`, `-bottom`, `-left` (CSS px;
  0 off mobile). Sheets and toasts in `zgui-ui` pad by them; demo `.lab` does too.
- **Keyboard.** `--zgui-keyboard-height` holds software keyboard height. Demo pads its bottom by
  `max(safe-area-bottom, keyboard-height)`; when keyboard grows, runtime scrolls focused element
  into view.
- **Back.** Back closes innermost dialog, sheet or popover (same as Escape). Unhandled Back moves
  app to background, as stock Android apps do.
- **Theme and text size.** Host reports system dark mode (as `prefers-color-scheme` and
  `ColorScheme::System`) and font scale (as `--zgui-text-scale`). `zgui-ui` type tokens scale by it.
- **Device services.** `zgui::app::android`: `notify`, `haptic`, `share`, `open_url`, `on_tilt`,
  `pick_image`. Callbacks run off the UI thread; reach UI state with `ui().post`.
- **Touch capture.** A pointer captured by an element (slider, drawing surface) drags that element
  and no longer pans the scroller under it.
- **System bar icons** follow system theme; `android::system_bar_theme` sets the theme the app
  shows instead. Demo calls it from its theme button.
- **Right to left.** An RTL system language sets `direction: rtl` on the root. Widgets tab has
  Arabic and Hebrew samples (system Noto fonts) with a switch that mirrors the card; `Slider` runs
  from the right edge in RTL (`NodeRef::is_rtl`).
- **TalkBack.** `accesskit_android` injects into the NativeActivity content view and receives
  zgui's AccessKit tree: swipe navigation, reading and double-tap activation. Touch exploration does
  not work yet: winit consumes hover events before any view sees them.
- **Haptics** follow Android's touch-feedback setting; with it off, Android drops them.
- **Tilt** needs no permission on stock Android; GrapheneOS asks for its Sensors permission.
- **Refresh rate.** Host requests display's fastest mode and reports rate in effect; runtime paces
  animation by it. Smooth Display off caps phone at 60 Hz; on, Pixel 9 runs app at 120 Hz.

## Activity lifecycle

winit allows one event loop per process, and NativeActivity's `onDestroy` waits for the native
thread to return. On Android 10+ the host reports destruction before `onDestroy`; the loop ends
and `run_android` exits the process, so reopening right after closing starts a new process. If
Android still creates an activity in a process holding an old loop (Android 8–9, task cleared),
`run_android` exits and Android starts the activity again in a new process. State resets, as on
process death. Handled `config_changes` (rotation, dark mode, font scale, locale,
density and others) keep the activity alive, so those changes keep state.

## Host class

`crates/zgui-platform-winit/android/dev/zgui/android/ZguiHost.java` compiles to committed
`zgui-host.dex`. Only library maintainers rebuild it, after changing the Java source:

```bash
crates/zgui-platform-winit/android/build-dex.sh
```

## Device findings

Pixel 9, Android API 37, Mali-G715 Vulkan driver:

- Native controls, software keyboard display, real on-screen Q key, injected text input and reactive binding work.
- Slider drag changes bound progress; 56 rows insert, reverse and scroll by finger.
- Theme change, accordion, CSS animation and landscape layout render.
- Modal with blur and vector close icon works with renderer workaround below.
- Added 50 rows, reversed, completed new row, backgrounded/resumed, rotated and switched tabs: 1 of 56 complete retained.
- Headless regressions cover keyed identity and new rows surviving document-owner disposal.

### Keyboard dismissal and long lists

Caret reports update geometry without repeating Android's show-keyboard request. Hiding the
software keyboard stays effective until a field is explicitly pressed or text input activates.
Tapping the already-focused field requests reopening independently of caret updates.

Checklist uses existing virtualizer with fixed 60 CSS-pixel rows and four-row overscan. List
reads borrow task data and clone only visible rows. Headless test with 5,000 tasks verifies at
most 16 mounted rows and correct IDs/completion after scrolling.

### Vello Mali-G715 workaround

Opening first vector icon originally lost Vulkan device. Vello 0.9 unchecked preprocessing
shaders contain an out-of-bounds workgroup index on this GPU family; see
[upstream issue #1816](https://github.com/linebender/vello/issues/1816).
Vello CPU preprocessing avoids those shaders, but Vello 0.9 sizes its intermediate buffers at
fixed values (about 165 MB). CPU mode allocates, fills and uploads them on each vector frame:
about 17 ms per frame, and about 60 ms on first frame after idle. ZGUI now selects coverage
rasteriser for Android adapters named Mali-G715. Coverage fills gradient paths flat and uses
16-sample edges; CSS gradients, shadows and backdrop blur do not use this path. Other adapters
retain Vello.

Local build tools installed under ignored `target/android-toolchain/`. In this checkout:

```bash
source target/android-toolchain/env.sh
cargo apk build -p zgui-examples --example android-counter
adb devices
adb install --no-incremental -r target/debug/apk/examples/zgui-counter.apk
adb shell am start -n dev.zgui.counter/android.app.NativeActivity
```

APK: `target/debug/apk/examples/zgui-counter.apk`. Packaged native symbols stripped;
unstripped library remains under `target/aarch64-linux-android/debug/examples/`.
Debug signing key stays under `target/android-toolchain/user/`. `cargo clean` removes this
local tool installation and key; reinstall tools or keep SDK elsewhere for ongoing development.

Example name deliberately differs from `android`: that name would produce `libandroid.so`,
colliding with Android's system library of same name.
