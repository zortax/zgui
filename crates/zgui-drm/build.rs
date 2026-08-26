//! Reads the vendored uapi headers into `OUT_DIR/uapi.rs`.
//!
//! The headers under `uapi/` are copied from the Linux kernel at version 7.0.
//!
//! The alternative is transcribing the structs by hand. They are stable and portable enough that
//! it would work — DRM declares user pointers `__u64` so that every struct is layout-identical on
//! 32-bit and 64-bit, and the headers use fixed-width types throughout — but an ioctl request
//! number is computed from `size_of`, so one field of the wrong width silently produces a
//! different request number. Generating removes that class of fault rather than testing for it.

use std::env;
use std::path::PathBuf;

fn main() {
    for header in ["drm.h", "drm_mode.h", "drm_fourcc.h", "wrapper.h"] {
        println!("cargo::rerun-if-changed=uapi/{header}");
    }

    // Cargo compiles and runs a build script for every member on every host, so this one runs on
    // macOS and Windows too. `CARGO_CFG_TARGET_OS` is the platform being built *for*, which is the
    // question that matters: the headers are Linux's, and reading them anywhere else would fail
    // for a crate whose contents are compiled out on that platform anyway. Reading the target
    // rather than `cfg!(target_os)` is what makes this correct when cross-compiling, because a
    // build script's own `cfg` describes the host that runs it.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }

    let bindings = bindgen::Builder::default()
        .header("uapi/wrapper.h")
        .clang_arg("-Iuapi")
        // The kernel names everything in this interface `drm_*` or `DRM_*`. Allowlisting rather
        // than blocklisting keeps whatever the C library's own headers drag in — and they do drag
        // things in, through `linux/types.h` — out of a module that is meant to be one interface.
        .allowlist_type("drm_.*")
        .allowlist_var("DRM_.*")
        .allowlist_var("drm_.*")
        // The C comments carry indented prose, and rustdoc reads an indented block inside a doc
        // comment as Rust to compile. `drm_fourcc.h` alone breaks `cargo test --doc` that way.
        // The headers stay the place to read about this interface.
        .generate_comments(false)
        // `Default` is what lets a caller zero a request struct and fill in the two fields that
        // matter, which is how almost every one of these ioctls is used.
        .derive_default(true)
        .derive_debug(true)
        .derive_copy(true)
        // The generated layout assertions hold the compiler to the size, the alignment and every
        // field offset of every struct, which no hand-written test would. They sit beside the
        // sizes asserted by hand in `sys.rs`, and they fail the build rather than a test.
        .layout_tests(true)
        .use_core()
        .generate()
        .expect("the vendored DRM headers parse");

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    bindings
        .write_to_file(out.join("uapi.rs"))
        .expect("the generated bindings are written");
}
