//! What the session binaries ask the machine directly.
//!
//! `cargo xtask ledger ignored` forbids switching a test off, and states the alternative: a test
//! that needs something the machine may not have looks for it, reports on standard error that it
//! did not find it, and returns. Every question below is asked of the machine rather than of
//! `zgui_platform_drm::session`, because a binary that asked the subject whether its own
//! precondition holds would skip itself exactly when the subject broke.
//!
//! A message that names no remedy is the same as no message, so every refusal built on these says
//! what to do about it.

// This module is compiled into each session test binary, and each one uses the part of it that its
// own subject needs. So a helper is dead code in the binaries that do not call it, which says
// nothing about the workspace.
#![allow(dead_code)]
// The loader and two resource limits are reached through the C library, which the crate under test
// is on the unsafe ledger's allowlist for its own reasons. Every block below states what makes it
// sound.
#![allow(unsafe_code)]

use std::ffi::{CString, c_char, c_int, c_void};
use std::fs;
use std::path::PathBuf;

/// The two names libseat is installed under, written out.
///
/// Deliberately apart from `zgui_seat::library::SONAMES`. This is what the machine is asked about,
/// so a wrong `SONAMES` cannot decide that the machine has no libseat and send a binary into the
/// silent arm.
const INSTALLED_AS: [&str; 2] = ["libseat.so.1", "libseat.so"];

/// Where the kernel puts display devices.
const DEVICES: &str = "/dev/dri";

/// Returns `true` if libseat opens on this machine, asked of the loader directly.
///
/// The library is opened and closed again. A handle the loader answers is a library a seat can be
/// opened through: the seated binary needs one, and the direct binary asserts its absence.
pub(crate) fn libseat_is_installed() -> bool {
    INSTALLED_AS.into_iter().any(|soname| {
        let Ok(soname) = CString::new(soname) else {
            return false;
        };

        // SAFETY: `soname` is a NUL-terminated string that stands for the length of the call.
        // Opening a shared object runs the initialisers of its whole dependency closure, which for
        // libseat is the C library and libsystemd; libseat does no work of its own at load time.
        let handle = unsafe { dlopen(soname.as_ptr(), RTLD_LAZY) };

        if handle.is_null() {
            return false;
        }

        // SAFETY: the handle the call above answered, closed once, here. Nothing was resolved out
        // of it, so nothing points into the mapping.
        unsafe { dlclose(handle) };
        true
    })
}

/// Runs `work` in a process that can open no file at all.
///
/// This is how a library is put out of reach without touching the machine. `dlopen` opens the
/// shared object it is asked for, so a process whose descriptor limit is zero resolves no soname:
/// the loader answers "too many open files" for every name, and a caller sees what it would see on
/// a machine where the library was never installed.
///
/// The limit is put back before anything is asserted, because a process that may open no descriptor
/// cannot report a failure either.
///
/// # A library that is already mapped
///
/// The loader answers a name it has already mapped out of its own list and opens nothing. So this
/// puts a library out of reach for a process that has never reached it, and a binary that uses it
/// runs the subject before it probes for anything.
pub(crate) fn while_nothing_opens<T>(work: impl FnOnce() -> T) -> T {
    let limit = descriptor_limit();

    set_descriptor_limit(&Rlimit {
        current: 0,
        maximum: limit.maximum,
    });
    let answer = work();
    set_descriptor_limit(&limit);

    answer
}

/// Returns the `card*` devices under `/dev/dri` this process can open for itself, sorted by path.
///
/// The walk is this module's own, and it opens each candidate the way the noop backend opens a
/// device: `O_RDWR`, and nothing else. So a card in this list is a card a seat on that backend
/// hands over, and a machine that answers an empty list is a machine where the seated card cannot
/// be asserted about at all.
pub(crate) fn openable_cards() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(DEVICES) else {
        return Vec::new();
    };

    let mut cards: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("card"))
        })
        .filter(|path| {
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .is_ok()
        })
        .collect();
    cards.sort();
    cards
}

/// Returns how many descriptors this process holds.
///
/// `/proc/self/fd` carries one entry per descriptor, and reading it holds one of its own, so the
/// answer is one higher than the number held. Every call here counts the same way and every
/// comparison is between two of them.
pub(crate) fn open_descriptors() -> usize {
    fs::read_dir("/proc/self/fd")
        .unwrap_or_else(|error| {
            panic!("this backend runs on Linux, which has `/proc/self/fd` to count: {error}")
        })
        .count()
}

/// `RTLD_LAZY`, which resolves a symbol when it is first called.
///
/// Nothing is resolved out of the handle here, so this is the cheapest open the loader offers.
const RTLD_LAZY: c_int = 1;

// The loader's own, for asking whether a library is here. Declared here for the same reason
// `zgui-seat` declares libseat's interface by hand: what crosses is stated once, beside the code
// that calls it.
unsafe extern "C" {
    /// `dlopen(3)`. Answers a handle on the shared object, or null.
    fn dlopen(file: *const c_char, flags: c_int) -> *mut c_void;
    /// `dlclose(3)`. Gives one handle back, and answers `0` or non-zero.
    fn dlclose(handle: *mut c_void) -> c_int;
}

/// Linux's `RLIMIT_NOFILE`: how many descriptors this process may hold.
///
/// Written out, because the standard library carries no resource limits and this binary names no
/// crate that does. `7` is the kernel's generic numbering, which every architecture this suite runs
/// on uses. A few older ones number their limits their own way.
const DESCRIPTOR_LIMIT: c_int = 7;

/// The C library's `struct rlimit`.
///
/// `rlim_t` is eight bytes wide on the 64-bit targets this suite runs on.
#[repr(C)]
#[derive(Clone, Copy)]
struct Rlimit {
    /// The limit in force. A process lowers this for itself and raises it again up to `maximum`.
    current: u64,
    /// The ceiling on `current`. Only a privileged process raises this one.
    maximum: u64,
}

// The C library's own, which is where a resource limit is read and written. Both are declared here
// for the reason the loader's two are.
unsafe extern "C" {
    /// `getrlimit(2)`. Writes the limit in force through the pointer, and answers `0` or `-1`.
    fn getrlimit(resource: c_int, limit: *mut Rlimit) -> c_int;
    /// `setrlimit(2)`. Puts the limit behind the pointer in force, and answers `0` or `-1`.
    fn setrlimit(resource: c_int, limit: *const Rlimit) -> c_int;
}

/// Returns the descriptor limit this process holds.
fn descriptor_limit() -> Rlimit {
    let mut limit = Rlimit {
        current: 0,
        maximum: 0,
    };

    // SAFETY: `getrlimit` writes one `struct rlimit` through the pointer, and this is one, owned by
    // this frame and live for the length of the call. The resource is a number the system defines.
    let answer = unsafe { getrlimit(DESCRIPTOR_LIMIT, &raw mut limit) };

    assert_eq!(answer, 0, "a process reads its own descriptor limit");
    limit
}

/// Puts a descriptor limit in force.
fn set_descriptor_limit(limit: &Rlimit) {
    // SAFETY: `setrlimit` reads one `struct rlimit` through the pointer, and this is one, live for
    // the length of the call. Lowering the limit in force, and raising it again up to the ceiling it
    // came with, is what a process does for itself.
    let answer = unsafe { setrlimit(DESCRIPTOR_LIMIT, limit) };

    assert_eq!(
        answer, 0,
        "a process lowers its own descriptor limit and puts it back"
    );
}
