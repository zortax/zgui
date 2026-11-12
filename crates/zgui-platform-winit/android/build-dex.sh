#!/usr/bin/env bash
# Rebuild zgui-host.dex after changing ZguiHost.java. Rust embeds the committed dex, so
# applications need no Java tools. Requires ANDROID_HOME with Platform 35 and Build-Tools 35.0.0.
set -euo pipefail
cd "$(dirname "$0")"
: "${ANDROID_HOME:?Set ANDROID_HOME to Android SDK}"
android_jar="$ANDROID_HOME/platforms/android-35/android.jar"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
javac --release 11 -classpath "$android_jar" -d "$work/classes" dev/zgui/android/ZguiHost.java
jar cf "$work/host.jar" -C "$work/classes" .
"$ANDROID_HOME/build-tools/35.0.0/d8" --release --min-api 26 --lib "$android_jar" \
    --output "$work" "$work/host.jar"
cp "$work/classes.dex" zgui-host.dex
