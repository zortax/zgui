#!/usr/bin/env bash
# Rebuild zgui.dex after changing the Java sources. Rust embeds the committed dex, and packaging
# copies it into the APK, so applications need no Java tools. Requires ANDROID_HOME with
# Platform 35 and Build-Tools 35.0.0.
set -euo pipefail
cd "$(dirname "$0")"
: "${ANDROID_HOME:?Set ANDROID_HOME to Android SDK}"
android_jar="$ANDROID_HOME/platforms/android-35/android.jar"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
javac --release 11 -Xlint:all -Xlint:-options -classpath "$android_jar" -d "$work/classes" dev/zgui/android/*.java
jar cf "$work/zgui.jar" -C "$work/classes" .
"$ANDROID_HOME/build-tools/35.0.0/d8" --release --min-api 26 --lib "$android_jar" \
    --output "$work" "$work/zgui.jar"
cp "$work/classes.dex" zgui.dex
