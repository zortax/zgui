//! Asking a card what it can do, without going through this crate.
//!
//! [`zgui_drm::Device::is_atomic`] answers what the device recorded when it was built, so a test
//! that reads it to decide whether to assert reports a defect in *that* code as a fact about the
//! machine, and switches itself off. This asks the kernel the same question over a descriptor of
//! its own, so the two answers can be compared.
//!
//! The request number is computed here, from the macro the header states it with. A number taken
//! out of the crate under test would agree with a wrong one rather than expose it.

use std::path::Path;

use rustix::fd::{AsFd, BorrowedFd};
use rustix::fs::{Mode, OFlags};
use rustix::io::Errno;
use rustix::ioctl::{Opcode, Setter};

/// The argument of `DRM_IOCTL_SET_CLIENT_CAP`.
///
/// `struct drm_set_client_cap` of `uapi/drm.h`. Both fields are `__u64`, so the struct is 16 bytes
/// wide on every architecture, and the request number is computed over that width.
#[repr(C)]
struct SetClientCap {
    /// Which capability to set.
    capability: u64,
    /// What to set it to.
    value: u64,
}

/// The width the request number below is computed over.
const _: () = assert!(size_of::<SetClientCap>() == 16);

/// `DRM_IOCTL_BASE`: the character DRM groups its request numbers under.
const GROUP: u32 = b'd' as u32;

/// The number `DRM_IOCTL_SET_CLIENT_CAP` is `DRM_IOW(0x0d, struct drm_set_client_cap)` at.
const NUMBER: u32 = 0x0d;

/// `_IOC_WRITE`: the kernel reads the argument and writes nothing back through it.
const WRITE: u32 = 1;

/// `DRM_IOCTL_SET_CLIENT_CAP`, spelled out.
///
/// `_IOC(direction, type, nr, size)` puts the direction at bit 30, the size at bit 16, the group
/// character at bit 8 and the number at bit 0.
const SET_CLIENT_CAP: Opcode =
    ((WRITE << 30) | ((size_of::<SetClientCap>() as u32) << 16) | (GROUP << 8) | NUMBER) as Opcode;

/// What the kernel numbers `DRM_CLIENT_CAP_UNIVERSAL_PLANES`.
const UNIVERSAL_PLANES: u64 = 2;

/// What the kernel numbers `DRM_CLIENT_CAP_ATOMIC`.
const ATOMIC: u64 = 3;

/// Returns `true` if the card at `path` takes `DRM_CLIENT_CAP_ATOMIC`.
///
/// The card is opened again here. A client capability is recorded on the open file description, so
/// this answers over a description of its own and changes nothing about a device the caller holds.
/// It needs no DRM master and no privilege, so it answers under a running compositor.
///
/// # Panics
///
/// Panics when the card cannot be opened, and when it refuses
/// `DRM_CLIENT_CAP_UNIVERSAL_PLANES`. Every modesetting driver has taken that one since kernel
/// 3.17, so a refusal means the request number above is wrong. A probe with a wrong number answers
/// `false` for every card there is, and that is the case the control below exists for.
pub(crate) fn takes_atomic(path: &Path) -> bool {
    let card = rustix::fs::open(path, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())
        .unwrap_or_else(|errno| {
            panic!(
                "a card that opened once opens again: {} answers {errno}",
                path.display()
            )
        });

    // The control. `drm_setclientcap` takes universal planes from any client of a modesetting
    // device, and `/dev/dri/card*` is one. So this call succeeding says the number reaches the
    // kernel call it was computed for, and whatever the atomic capability then answers is the
    // driver speaking.
    if let Err(errno) = set(card.as_fd(), UNIVERSAL_PLANES) {
        panic!(
            "{} refused DRM_CLIENT_CAP_UNIVERSAL_PLANES over request {SET_CLIENT_CAP:#010x}: \
             {errno}\n\
             every modesetting card takes that capability, so the request number computed in this \
             module is wrong, and every card would read here as having no atomic interface",
            path.display()
        );
    }

    set(card.as_fd(), ATOMIC).is_ok()
}

/// Asks the kernel to turn `capability` on for `fd`.
///
/// `EINTR` is retried, as libdrm's own `drmIoctl` does. A refusal that came from a signal would
/// otherwise read as a card with no atomic interface.
fn set(fd: BorrowedFd<'_>, capability: u64) -> Result<(), Errno> {
    loop {
        let request = SetClientCap {
            capability,
            value: 1,
        };

        // SAFETY: `SET_CLIENT_CAP` is `_IOW('d', 0x0d, struct drm_set_client_cap)`, computed above
        // over `SetClientCap`, which is that struct. So the kernel reads 16 bytes of a live value
        // of exactly the layout the number was derived from, and writes nothing back. `fd` is a
        // live borrowed descriptor for the whole call.
        let outcome = unsafe {
            rustix::ioctl::ioctl(fd, Setter::<SET_CLIENT_CAP, SetClientCap>::new(request))
        };

        match outcome {
            Err(Errno::INTR) => continue,
            outcome => return outcome,
        }
    }
}
