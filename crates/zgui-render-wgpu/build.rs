//! Compiles every shading module before the program that runs them exists.
//!
//! A shader reaches a driver as an intermediate representation, and turning WGSL text into one is
//! lexing, parsing, resolving and lowering a few thousand lines — the same few thousand lines, to
//! the same representation, on every launch. It is done here instead, once per build, and a launch
//! loads the result. A module that does not parse fails the build rather than the program.

use std::path::{Path, PathBuf};

/// The files each shading module is built from, in order, under the name it is known by.
///
/// The shared pieces are concatenated rather than imported, because a shading module is one
/// translation unit: a function defined once and used by six pipelines is one piece of text
/// compiled six times, which is what makes a clip on a quad and a clip on a glyph provably the
/// same clip rather than two implementations that agree today.
const MODULES: &[(&str, &[&str])] = &[
    ("quad", &["common", "sdf", "paint", "quad"]),
    ("shadow", &["common", "sdf", "paint", "shadow"]),
    ("decoration", &["common", "sdf", "paint", "decoration"]),
    (
        "mono_sprite",
        &["common", "sdf", "paint", "text", "sprite", "sprite_mono"],
    ),
    (
        "color_sprite",
        &["common", "sdf", "paint", "text", "sprite", "sprite_color"],
    ),
    (
        "subpixel_sprite",
        &["common", "sdf", "paint", "text", "sprite", "subpixel"],
    ),
    // The copy reads one texture and nothing else, so it shares none of the above and its
    // bind-group layout is its own.
    ("blit", &["blit"]),
    // A clear draws a colour and reads nothing at all.
    ("clear", &["clear"]),
    // The blur chain reads one texture through one block; clips and paints belong to the composite
    // that follows it, not to the filtering itself.
    ("blur", &["blur"]),
    ("composite", &["common", "sdf", "composite"]),
    ("external", &["common", "sdf", "external"]),
    ("vector", &["common", "sdf", "vector"]),
];

/// What has to precede every other item of the module that blends against a second colour output.
///
/// An extension is declared before anything else in a translation unit, which is why it is written
/// here rather than at the top of the file that needs it.
const PRELUDE: &[(&str, &str)] = &[("subpixel_sprite", "enable dual_source_blending;")];

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

    let sources = Path::new("src/shader");
    println!("cargo::rerun-if-changed={}", sources.display());
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo names an output directory"));

    for (name, parts) in MODULES {
        let mut pieces: Vec<String> = PRELUDE
            .iter()
            .filter(|(module, _)| module == name)
            .map(|(_, text)| (*text).to_owned())
            .collect();
        for part in *parts {
            let path = sources.join(format!("{part}.wgsl"));
            println!("cargo::rerun-if-changed={}", path.display());
            pieces.push(
                std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("{}: {error}", path.display())),
            );
        }
        let source = pieces.join("\n");

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
