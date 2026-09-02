//! Compiles every shading module before the program that runs them exists.
//!
//! A shader reaches a driver as an intermediate representation, and turning WGSL text into one is
//! lexing, parsing, resolving and lowering a few thousand lines — the same few thousand lines, to
//! the same representation, on every launch. It is done here instead, once per build, and a launch
//! loads the result. A module that does not parse fails the build rather than the program.
//!
//! The text itself lives in `zgui-wgsl`, along with the table saying which pieces each module is
//! concatenated from. It is one copy, shared with the macro that compiles an application's own
//! shader, so the two ends cannot drift apart.

use std::path::{Path, PathBuf};

/// Declares `vulkan_hal`, and sets it where wgpu links a Vulkan backend to reach through its hal.
///
/// The Vulkan backend arrives through `wgpu-core-deps-windows-linux-android`, which wgpu-core
/// depends on under `cfg(any(windows, target_os = "linux", target_os = "android", target_os =
/// "freebsd"))` — the four targets below, read out of the manifest of wgpu-core 29.0.4. Elsewhere
/// that crate is no dependency of wgpu-core, so `wgpu::hal::api::Vulkan` and `wgpu::hal::vulkan`
/// need not exist and code naming them need not compile. The condition is stated once here, so
/// that `src/gpu/extensions.rs` carries one short `cfg`.
fn declare_vulkan_hal() {
    println!("cargo::rustc-check-cfg=cfg(vulkan_hal)");
    let target =
        std::env::var("CARGO_CFG_TARGET_OS").expect("cargo names the target it builds for");
    if matches!(target.as_str(), "windows" | "linux" | "android" | "freebsd") {
        println!("cargo::rustc-cfg=vulkan_hal");
    }
}

/// Concatenates every module, parses it, and writes both the text and the representation out.
fn main() {
    declare_vulkan_hal();

    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo names an output directory"));

    for (name, _) in zgui_wgsl::MODULES {
        let source = zgui_wgsl::module(name).expect("a listed module assembles");

        let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|error| {
            panic!("{name}: {}", error.emit_to_string(&source));
        });
        let ir = bincode::serialize(&module)
            .unwrap_or_else(|error| panic!("{name}: the representation would not encode: {error}"));

        write(&out.join(format!("{name}.wgsl")), source.as_bytes());
        write(&out.join(format!("{name}.ir")), &ir);
    }
}

/// Writes one generated file, naming it if it cannot be written.
fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
}
