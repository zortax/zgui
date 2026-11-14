#!/usr/bin/env bash
# Build the Rust cdylib and package it with ZGUI's Java classes. Needs no Java compiler, Gradle
# or cargo-apk.
set -euo pipefail
cd "$(dirname "$0")/../.."
: "${ANDROID_HOME:?Set ANDROID_HOME to Android SDK}"
profile="${1:-debug}"
case "$profile" in debug|release) ;; *) echo 'Usage: package.sh [debug|release]' >&2; exit 2 ;; esac
android_jar="$ANDROID_HOME/platforms/android-35/android.jar"
build_tools="$ANDROID_HOME/build-tools/35.0.0"
args=()
if [[ "$profile" == release ]]; then args+=(--release); fi
: "${ANDROID_NDK_HOME:?Set ANDROID_NDK_HOME to Android NDK}"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android26-clang"
export CC_aarch64_linux_android="$CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"
export AR_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-ar"
cargo build -p zgui-examples --example android-counter --target aarch64-linux-android "${args[@]}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
# ZGUI ships its activity, service, receivers and provider prebuilt.
cp crates/zgui-platform-winit/android/zgui.dex "$work/classes.dex"
mkdir -p "$work/lib/arm64-v8a"
cp "target/aarch64-linux-android/$profile/examples/libandroid_counter.so" "$work/lib/arm64-v8a/"
"$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip" "$work/lib/arm64-v8a/libandroid_counter.so"
"$build_tools/aapt" package -f -M examples/android/AndroidManifest.xml -S examples/android/res -I "$android_jar" -F "$work/lab.apk"
# Align uncompressed native library entries for Android devices with 16 KiB pages.
python3 - "$work" <<'PY'
import pathlib, sys, zipfile
root = pathlib.Path(sys.argv[1])
with zipfile.ZipFile(root / 'lab.apk', 'a') as apk:
    apk.write(root / 'classes.dex', 'classes.dex', compress_type=zipfile.ZIP_DEFLATED)
    for lib in sorted((root / 'lib').rglob('*.so')):
        apk.write(lib, str(lib.relative_to(root)), compress_type=zipfile.ZIP_STORED)
PY
mkdir -p "target/$profile/apk/examples"
output="target/$profile/apk/examples/zgui-mobile-lab.apk"
"$build_tools/zipalign" -f -P 16 4 "$work/lab.apk" "$output"
keystore="${ZGUI_ANDROID_KEYSTORE:-target/android-toolchain/debug.keystore}"
if [[ ! -f "$keystore" ]]; then
    mkdir -p "$(dirname "$keystore")"
    keytool -genkeypair -keystore "$keystore" -storepass "${ZGUI_ANDROID_KEYSTORE_PASSWORD:-android}" -keypass "${ZGUI_ANDROID_KEYSTORE_PASSWORD:-android}" -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 -dname 'CN=Android Debug,O=Android,C=US'
fi
# Local debug certificate only; release publishing needs separate signing workflow.
export ZGUI_ANDROID_KEYSTORE_PASSWORD="${ZGUI_ANDROID_KEYSTORE_PASSWORD:-android}"
"$build_tools/apksigner" sign --ks "$keystore" --ks-pass env:ZGUI_ANDROID_KEYSTORE_PASSWORD "$output"
"$build_tools/apksigner" verify "$output"
echo "$output"
