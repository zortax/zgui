# Android Mobile Lab

Bring-up target: ARM64 Android phone, Android 8.0 / API 26 or newer.
NativeActivity loads Rust `cdylib`; ZGUI renders shipped components with bundled font.

- **Controls:** editable name with live greeting, switch, checkbox, slider, progress,
  tap counter, disabled button, animated modal with blurred backdrop and vector dismiss icon.
- **Tasks:** add/remove/complete, derived count, clear completed, reverse keyed rows,
  add 50 stress rows, finger scrolling and scrollbar.
- **Visuals:** live light/dark theme, gradients, grid, shadows, clipping, CSS transform
  animation (off initially), accordion and responsive landscape layout.

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

Also requires JDK (`java` and `keytool` on PATH). NDK path must name version directory,
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

Enable USB debugging on phone, connect USB, approve host, then:

```bash
adb devices
cargo apk run -p zgui-examples --example android-counter
```

Packaging follows [cargo-apk's NativeActivity example workflow](https://github.com/rust-mobile/cargo-apk).

## Device acceptance checks

1. Controls: edit name, confirm greeting and character count; toggle switch and checkbox.
2. Drag slider: numeric value and progress move together. Disabled button stays inert.
3. Open dialog: blurred backdrop, vector close icon and Done button render; dismiss works.
4. Tasks: add text, complete rows, clear done, reverse order; add 50 and scroll by finger.
5. Visuals: expand accordion, enable motion, change theme; animation moves without input.
6. Rotate, Home, return, switch tabs: names, row count, completion, slider and theme survive.
7. Disable motion and leave editable fields unfocused before measuring idle work.

Count and edits remain in process memory. OS process death resets state.
Clipboard and Android accessibility adapter remain unavailable. Keyboard occlusion, native
Back navigation and system-bar appearance need further mobile integration.

## Device findings

Pixel 9, Android API 37, Mali-G715 Vulkan driver:

- Native controls, software keyboard display, real on-screen Q key, injected text input and reactive binding work.
- Slider drag changes bound progress; 56 rows insert, reverse and scroll by finger.
- Theme change, accordion, CSS animation and landscape layout render.
- Modal with blur and vector close icon works with renderer workaround below.
- Added 50 rows, reversed, completed new row, backgrounded/resumed, rotated and switched tabs: 1 of 56 complete retained.
- Headless regressions cover keyed identity and new rows surviving document-owner disposal.

### Vello Mali-G715 workaround

Opening first vector icon originally lost Vulkan device. Vello 0.9 unchecked preprocessing
shaders contain an out-of-bounds workgroup index on this GPU family; see
[upstream issue #1816](https://github.com/linebender/vello/issues/1816).
ZGUI now selects Vello CPU preprocessing for Android adapters named Mali-G715. Fine
rasterization and composition remain on GPU. This costs CPU time for vector content;
other adapters retain existing path. Full backdrop effects remain enabled.

Local build tools installed under ignored `target/android-toolchain/`. In this checkout:

```bash
source target/android-toolchain/env.sh
cargo apk build -p zgui-examples --example android-counter
adb devices
cargo apk run -p zgui-examples --example android-counter
```

APK: `target/debug/apk/examples/zgui-counter.apk`. Packaged native symbols stripped;
unstripped library remains under `target/aarch64-linux-android/debug/examples/`.
Debug signing key stays under `target/android-toolchain/user/`. `cargo clean` removes this
local tool installation and key; reinstall tools or keep SDK elsewhere for ongoing development.

Example name deliberately differs from `android`: that name would produce `libandroid.so`,
colliding with Android's system library of same name.
