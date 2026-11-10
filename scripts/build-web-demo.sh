#!/usr/bin/env bash
set -euo pipefail

if [[ $# -gt 1 ]]; then
    printf 'Usage: %s [output-directory]\n' "$0" >&2
    exit 2
fi
if [[ "${1:-}" == '-h' || "${1:-}" == '--help' ]]; then
    printf 'Usage: %s [output-directory]\n' "$0"
    printf 'Build the browser demo into target/web-demo, or the supplied directory.\n'
    exit 0
fi

ZGUI_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
ZGUI_OUTPUT="${1:-"$ZGUI_ROOT/target/web-demo"}"
if [[ "$ZGUI_OUTPUT" != /* ]]; then
    ZGUI_OUTPUT="$PWD/$ZGUI_OUTPUT"
fi
ZGUI_BINDGEN_REQUIRED='0.2.126'
ZGUI_FONT_LICENSE="$ZGUI_ROOT/crates/zgui-text-parley/tests/fonts/OFL-NotoSans.txt"

if ! command -v cargo >/dev/null 2>&1; then
    printf 'cargo is required. Install Rust with rustup, then install the stable toolchain.\n' >&2
    exit 1
fi
if ! command -v wasm-bindgen >/dev/null 2>&1; then
    printf 'Install the bindings generator: cargo +stable install wasm-bindgen-cli --version %s --locked\n' \
        "$ZGUI_BINDGEN_REQUIRED" >&2
    exit 1
fi
ZGUI_BINDGEN_ACTUAL="$(wasm-bindgen --version)"
if [[ "$ZGUI_BINDGEN_ACTUAL" != "wasm-bindgen $ZGUI_BINDGEN_REQUIRED" ]]; then
    printf 'Expected wasm-bindgen %s; found %s.\n' \
        "$ZGUI_BINDGEN_REQUIRED" "$ZGUI_BINDGEN_ACTUAL" >&2
    printf 'Install the matching CLI: cargo +stable install wasm-bindgen-cli --version %s --locked\n' \
        "$ZGUI_BINDGEN_REQUIRED" >&2
    exit 1
fi
for ZGUI_INPUT in "$ZGUI_ROOT/web/index.html" "$ZGUI_ROOT/LICENSE" "$ZGUI_ROOT/NOTICE" "$ZGUI_FONT_LICENSE"; do
    if [[ ! -f "$ZGUI_INPUT" ]]; then
        printf 'Required package file is missing: %s\n' "$ZGUI_INPUT" >&2
        exit 1
    fi
done

cd -- "$ZGUI_ROOT"
cargo +stable build --locked --profile web --target wasm32-unknown-unknown \
    -p zgui-web-demo --target-dir "$ZGUI_ROOT/target"

mkdir -p -- "$ZGUI_OUTPUT/pkg"
wasm-bindgen "$ZGUI_ROOT/target/wasm32-unknown-unknown/web/zgui_web_demo.wasm" \
    --target web --out-dir "$ZGUI_OUTPUT/pkg" --out-name zgui_web_demo
cp -- "$ZGUI_ROOT/web/index.html" "$ZGUI_OUTPUT/index.html"
cp -- "$ZGUI_ROOT/LICENSE" "$ZGUI_ROOT/NOTICE" "$ZGUI_FONT_LICENSE" "$ZGUI_OUTPUT/"

printf 'Browser demo written to %s\n' "$ZGUI_OUTPUT"
printf 'Serve this directory over HTTP or HTTPS; see docs/guide/wasm.md.\n'
