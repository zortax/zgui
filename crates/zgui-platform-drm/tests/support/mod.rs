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
// The loader, two resource limits and one system call the C library carries no wrapper for are
// reached through that library, which the crate under test is on the unsafe ledger's allowlist for
// its own reasons. Every block below states what makes it sound.
#![allow(unsafe_code)]

use std::ffi::{CString, c_char, c_int, c_long, c_void};
use std::fs;
use std::os::fd::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};

/// The two names libseat is installed under, written out.
///
/// Deliberately apart from `zgui_seat::library::SONAMES`. This is what the machine is asked about,
/// so a wrong `SONAMES` cannot decide that the machine has no libseat and send a binary into the
/// silent arm.
const INSTALLED_AS: [&str; 2] = ["libseat.so.1", "libseat.so"];

/// Where the kernel puts display devices.
const DEVICES: &str = "/dev/dri";

/// Where the kernel puts input devices.
const NODES: &str = "/dev/input";

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
/// This is how a seat is put out of reach without touching the machine. Every step towards one
/// opens something: `dlopen` opens the shared object it is asked for, and libseat's noop backend
/// makes a `socketpair` for the seat it answers. A process whose descriptor limit is zero gets
/// "too many open files" from whichever of them it reaches first, which is a failure the caller has
/// to answer for.
///
/// The limit is put back before anything is asserted, because a process that may open no descriptor
/// cannot report a failure either.
///
/// # A library that is already mapped
///
/// The loader answers a name it has already mapped out of its own list and opens nothing. glibc
/// unmaps a shared object when the last handle on it closes, so a process there reaches the loader
/// again; musl unmaps nothing, and a process that links libseat directly has it mapped from the
/// start. So a binary that needs the library out of reach runs its subject before it probes for
/// anything.
pub(crate) fn while_nothing_opens<T>(work: impl FnOnce() -> T) -> T {
    while_the_limit_is(0, work)
}

/// Runs `work` with `current` as this process's descriptor limit.
///
/// The limit is what the kernel refuses a new descriptor against: it allocates the lowest free
/// number below the limit, and answers "too many open files" where there is none. So a limit one
/// above [`lowest_free_descriptor`] leaves room for exactly one more descriptor, which is how a
/// second call that asks for one is made to fail while the first succeeds.
///
/// The limit is put back before `work`'s answer is looked at, for the reason
/// [`while_nothing_opens`] states.
pub(crate) fn while_the_limit_is<T>(current: u64, work: impl FnOnce() -> T) -> T {
    let limit = descriptor_limit();

    set_descriptor_limit(&Rlimit {
        current,
        maximum: limit.maximum,
    });
    let answer = work();
    set_descriptor_limit(&limit);

    answer
}

/// Returns the lowest descriptor number this process has free.
///
/// Asked by opening one and closing it again. The kernel hands out the lowest free number, so the
/// number it answered is that number, and it is free again by the time this returns.
pub(crate) fn lowest_free_descriptor() -> u64 {
    let file = fs::File::open("/dev/null")
        .unwrap_or_else(|error| panic!("every Linux machine has `/dev/null` to open: {error}"));
    let number = file.as_raw_fd();
    drop(file);

    u64::try_from(number).unwrap_or_else(|_| panic!("a descriptor number is never negative"))
}

/// Returns the descriptors this process holds that name `path`.
///
/// `/proc/self/fd` carries one symbolic link per descriptor, and each one reads back as what that
/// descriptor names. So this finds a descriptor by the path it was opened on, including one that
/// somebody else opened and handed over.
pub(crate) fn descriptors_naming(path: &Path) -> Vec<RawFd> {
    let mut found: Vec<RawFd> = fs::read_dir("/proc/self/fd")
        .unwrap_or_else(|error| {
            panic!("this backend runs on Linux, which has `/proc/self/fd` to read: {error}")
        })
        .filter_map(std::result::Result::ok)
        .filter(|entry| fs::read_link(entry.path()).is_ok_and(|named| named == path))
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse().ok())
        })
        .collect();
    found.sort_unstable();
    found
}

/// Returns `true` if `first` and `second` name one open file description.
///
/// This is the fact a duplicate exists for, and the one thing `/proc` cannot show: a descriptor
/// copied with `F_DUPFD_CLOEXEC` and a second `open(2)` of the same path read back as the same link
/// and are reported with the same position and the same flags, while the kernel holds DRM master,
/// the client capabilities and the status flags on the description behind them.
///
/// `kcmp(2)` with `KCMP_FILE` asks. The kernel compares the `struct file` behind each number and
/// answers an ordering, so zero is one description under two names.
///
/// `None` where this machine cannot answer: an architecture whose call number is not written out
/// below, or a kernel built without `CONFIG_CHECKPOINT_RESTORE`, which refuses the call. A caller
/// reports that and asserts nothing.
pub(crate) fn one_open_file_description(first: RawFd, second: RawFd) -> Option<bool> {
    let number = KCMP?;
    // A process id is a 32-bit number the kernel keeps well inside its own limit, so it crosses as
    // a `long` on every architecture, including the ones where a `long` is the narrower of the two.
    let process = std::process::id() as c_long;

    // SAFETY: `syscall` passes the numbers below to the kernel, which reads two process ids, a
    // comparison and two descriptor numbers. `kcmp` reads nothing through a pointer and writes
    // nothing back, so every argument is an integer and the return value is the only result.
    let answer = unsafe {
        syscall(
            number,
            process,
            process,
            KCMP_FILE,
            c_long::from(first),
            c_long::from(second),
        )
    };

    if answer < 0 {
        return None;
    }
    Some(answer == 0)
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

/// Returns the `event*` nodes under `/dev/input` this process can open for itself, in kernel
/// order.
///
/// The walk is this module's own, and it opens each candidate the way the noop backend opens a
/// device: `O_RDWR`, and nothing else. So a node in this list is a node a seat on that backend
/// hands over, and a machine that answers an empty list is a machine where the seated input path
/// cannot be asserted about at all.
///
/// **Nothing here is grabbed, and nothing that reads this grabs one.** A grab is exclusive and it
/// takes the device from the session for as long as it is held, so what a session binary asserts
/// about an input device is the handover rather than the taking.
pub(crate) fn openable_input_nodes() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(NODES) else {
        return Vec::new();
    };

    let mut nodes: Vec<(u32, PathBuf)> = entries
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter_map(|path| {
            let number = path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix("event"))
                .and_then(|number| number.parse().ok())?;
            Some((number, path))
        })
        .filter(|(_, path)| {
            fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .is_ok()
        })
        .collect();
    nodes.sort();
    nodes.into_iter().map(|(_, path)| path).collect()
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

/// `kcmp(2)`'s comparison for "these two descriptors", which is the only one asked for here.
const KCMP_FILE: c_long = 0;

/// `kcmp(2)`'s system call number on this architecture.
///
/// Linux numbers its calls per architecture and the C library exports no wrapper for this one, so
/// the number is written out. `x86_64` is 312, and the generic table every architecture added since
/// `aarch64` uses puts it at 272. An architecture outside both answers nothing, and the caller
/// reports that it could not ask.
const KCMP: Option<c_long> = if cfg!(target_arch = "x86_64") {
    Some(312)
} else if cfg!(any(
    target_arch = "aarch64",
    target_arch = "riscv64",
    target_arch = "loongarch64"
)) {
    Some(272)
} else {
    None
};

// The C library's own, for a call it carries no wrapper for. Declared here for the reason the
// loader's two are.
unsafe extern "C" {
    /// `syscall(2)`. Takes the call number and passes the rest to the kernel.
    fn syscall(number: c_long, ...) -> c_long;
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
