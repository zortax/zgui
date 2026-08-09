//! Reads the vendored uapi headers into `OUT_DIR/uapi.rs`.
//!
//! The headers under `uapi/` are copied from the Linux kernel at version 7.0.
//!
//! The alternative is transcribing the structs and the code tables by hand. `input-event-codes.h`
//! alone is about eight hundred constants, and an ioctl request number is computed from
//! `size_of`, so one field of the wrong width silently produces a different request number.
//! Generating removes both classes of fault rather than testing for them.

use std::env;
use std::path::{Path, PathBuf};

/// The headers copied from the kernel, in the order they include each other.
const HEADERS: [&str; 3] = ["input.h", "input-event-codes.h", "uinput.h"];

fn main() {
    for header in HEADERS.iter().chain(&["wrapper.h"]) {
        println!("cargo::rerun-if-changed=uapi/{header}");
    }

    // Cargo compiles and runs a build script for every member on every host, so this one runs on
    // macOS and Windows too. `CARGO_CFG_TARGET_OS` is the platform being built *for*, which is the
    // question that matters: the headers are Linux's, and reading them anywhere else would fail
    // for a crate whose contents are compiled out on that platform anyway. Reading the target
    // rather than `cfg!(target_os)` keeps this correct when cross-compiling, because a build
    // script's own `cfg` describes the host that runs it.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }

    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let include = stage(&out);

    let bindings = bindgen::Builder::default()
        .header("uapi/wrapper.h")
        .clang_arg(format!("-I{}", include.display()))
        // The kernel names everything in this interface `input_*`, `uinput_*`, `ff_*` or one of
        // the code prefixes below. Allowlisting rather than blocklisting keeps whatever the C
        // library's own headers drag in — and they do drag things in, through `sys/time.h` — out
        // of a module that is meant to be one interface.
        .allowlist_type("input_.*")
        .allowlist_type("uinput_.*")
        .allowlist_type("ff_.*")
        .allowlist_var("EV_.*")
        .allowlist_var("SYN_.*")
        .allowlist_var("KEY_.*")
        .allowlist_var("BTN_.*")
        .allowlist_var("REL_.*")
        .allowlist_var("ABS_.*")
        .allowlist_var("MSC_.*")
        .allowlist_var("SW_.*")
        .allowlist_var("LED_.*")
        .allowlist_var("SND_.*")
        .allowlist_var("REP_.*")
        .allowlist_var("FF_.*")
        .allowlist_var("BUS_.*")
        .allowlist_var("INPUT_PROP_.*")
        .allowlist_var("UINPUT_.*")
        // The C comments carry indented prose, and rustdoc reads an indented block inside a doc
        // comment as Rust to compile. The headers stay the place to read about this interface.
        .generate_comments(false)
        // `Default` lets a caller zero a request struct and fill in the two fields that matter,
        // which is how almost every one of these ioctls is used.
        .derive_default(true)
        .derive_debug(true)
        .derive_copy(true)
        // The generated layout assertions hold the compiler to the size, the alignment and every
        // field offset of every struct, which no hand-written test would. They sit beside the
        // sizes asserted by hand in `sys.rs`, and they fail the build rather than a test.
        .layout_tests(true)
        .use_core()
        .generate()
        .expect("the vendored input headers parse");

    bindings
        .write_to_file(out.join("uapi.rs"))
        .expect("the generated bindings are written");
}

/// Copies the vendored headers into `out/include/linux`, and reports the directory to search.
///
/// `uinput.h` names `input.h` as `<linux/input.h>`, which is how the kernel's own tree spells it.
/// Reproducing that one directory level keeps the vendored copy self-contained: without it the
/// include resolves against the host's installed headers, and the generated bindings would then
/// describe whatever kernel the build machine has instead of the copies under `uapi/`.
fn stage(out: &Path) -> PathBuf {
    let include = out.join("include");
    let linux = include.join("linux");
    std::fs::create_dir_all(&linux).expect("the staging directory is created");
    for header in HEADERS {
        std::fs::copy(Path::new("uapi").join(header), linux.join(header))
            .expect("the vendored header is staged");
    }
    include
}
