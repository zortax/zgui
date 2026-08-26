//! Reads the vendored uapi headers into `OUT_DIR/uapi.rs`.
//!
//! The headers under `uapi/` are copied from the Linux kernel at version 7.0.
//!
//! The alternative is transcribing the structs and the code tables by hand. `input-event-codes.h`
//! alone is about eight hundred constants, and an ioctl request number is computed from
//! `size_of`, so one field of the wrong width silently produces a different request number.
//! Generating removes both classes of fault rather than testing for them.
//!
//! The console headers make the same argument from the other side: `kd.h` holds each request
//! number whole, so reading the header is the only way to get one right.

use std::env;
use std::path::{Path, PathBuf};

/// The headers copied from the kernel, in the order they include each other.
const HEADERS: [&str; 8] = [
    "input.h",
    "input-event-codes.h",
    "uinput.h",
    "time.h",
    "time_types.h",
    "kd.h",
    "keyboard.h",
    "wait.h",
];

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
        // The console keymap interface. One struct, the request numbers under `KD`, the keyboard
        // modes and the keymap vocabulary under `K_`, the entry types under `KT_`, the modifier
        // bits under `KG_`, and the counts that bound a keycode and a map index. A console keymap
        // entry is read as a type and a value, and both vocabularies are the header's.
        .allowlist_type("kbentry")
        // The composite entries `wrapper.h` restates. bindgen writes an enumeration as constants
        // by default, and that is the form these want: they are values of a sixteen-bit field, and
        // the field holds far more than three.
        .allowlist_type("zgui_console_entry")
        // Their names are already the header's. Prepending the enumeration's name would spell one
        // of them `zgui_console_entry_ZGUI_K_HOLE`, which says `K_HOLE` twice over.
        //
        // The setting is global, and it is right only while `zgui_console_entry` is the one `enum`
        // in all eight headers. Nothing checks that. A header that brought another one would have
        // its members generated without their enumeration's name, so whoever adds a header reads
        // this line — `wrapper.h` says the same thing where the enumeration is written.
        .prepend_enum_name(false)
        .allowlist_var("KD.*")
        .allowlist_var("K_.*")
        .allowlist_var("KT_.*")
        .allowlist_var("KG_.*")
        .allowlist_var("NR_.*")
        .allowlist_var("MAX_NR_.*")
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
        // The one value this crate needs from outside the input headers: `EVIOCSCLOCKID` takes a
        // clock, and `linux/time.h` is where the kernel numbers them. Vendoring it keeps the rule
        // that no value here is transcribed.
        .allowlist_var("CLOCK_MONOTONIC")
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

    clock(&include, &out);
}

/// Reads `linux/time.h` into `OUT_DIR/clock.rs`, on its own.
///
/// `EVIOCSCLOCKID` takes a clock, and `linux/time.h` is where the kernel numbers them. It cannot
/// join the pass above: `input.h` includes the C library's `sys/time.h` for `struct timeval`, and
/// the kernel's header defines `timeval`, `itimerval` and `timezone` itself, so a translation unit
/// holding both does not compile. A second pass costs one more `include!` and keeps the rule that
/// no value in this crate is transcribed.
fn clock(include: &Path, out: &Path) {
    let bindings = bindgen::Builder::default()
        .header_contents("clock.h", "#include <linux/time.h>\n")
        .clang_arg(format!("-I{}", include.display()))
        .allowlist_var("CLOCK_MONOTONIC")
        .generate_comments(false)
        .use_core()
        .generate()
        .expect("the vendored time header parses");

    bindings
        .write_to_file(out.join("clock.rs"))
        .expect("the generated clock constant is written");
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
