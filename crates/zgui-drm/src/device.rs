//! An open DRM device, and what it can do.

use std::path::{Path, PathBuf};

use rustix::fd::{AsFd, BorrowedFd, OwnedFd};
use rustix::fs::{Mode, OFlags};

use crate::error::{Error, Result};
use crate::ioctl;
use crate::sys;

/// Where the kernel puts display devices.
const DIRECTORY: &str = "/dev/dri";

/// Which modesetting interface to drive a device through.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Interface {
    /// Atomic where the driver offers it, legacy where it does not.
    #[default]
    Preferred,
    /// The legacy interface, whatever the driver offers.
    ///
    /// Every atomic driver still serves the legacy ioctls: the kernel implements them over its own
    /// atomic helpers. Asking for this on a device that has both is how the legacy path is
    /// exercised on hardware.
    Legacy,
}

/// An open display device.
#[derive(Debug)]
pub struct Device {
    /// The open descriptor.
    fd: OwnedFd,
    /// Where it was opened from, for messages.
    path: PathBuf,
    /// Whether the kernel accepted the atomic client capability.
    atomic: bool,
}

impl Device {
    /// Opens the device at `path`, preferring the atomic interface.
    ///
    /// ```
    /// use zgui_drm::Device;
    ///
    /// let refused = Device::open("/dev/dri/card-no-such-device")
    ///     .expect_err("no machine has a card by that name");
    ///
    /// assert!(refused.to_string().contains("/dev/dri/card-no-such-device"));
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Open`] when the device cannot be opened.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(path, Interface::Preferred)
    }

    /// Opens the device at `path`, through `interface`.
    ///
    /// With [`Interface::Preferred`] the universal-planes and atomic client capabilities are asked
    /// for, and a kernel that refuses atomic is not an error: it means the legacy interface is what
    /// this device has. With [`Interface::Legacy`] neither is asked for, so the device behaves as
    /// it would for a client that predates them. [`Device::is_atomic`] reports which one every
    /// later call will use.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Open`] when the device cannot be opened.
    pub fn open_with(path: impl AsRef<Path>, interface: Interface) -> Result<Self> {
        let path = path.as_ref().to_owned();
        // `O_NONBLOCK` makes reading flip events a poll: the frame loop asks whether a flip has
        // completed and carries on when it has not.
        let fd = rustix::fs::open(
            &path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|errno| Error::Open {
            path: path.clone(),
            source: errno.into(),
        })?;

        let mut device = Self {
            fd,
            path,
            atomic: false,
        };

        // A caller that asked for the legacy interface asks for neither capability, so the kernel
        // presents the device the way it did before either existed.
        if interface == Interface::Preferred {
            // Universal planes has to be accepted before atomic, and asking for atomic implies it.
            // Asking for both separately is what tells the two failures apart on a kernel that has
            // one and not the other.
            //
            // A kernel that takes the first and refuses the second leaves the device with
            // universal planes on and `atomic` false, and a client capability cannot be turned
            // back off. That tier is a real one — legacy modesetting with the full plane list —
            // and the legacy path addresses a CRTC directly, so it neither reads the plane list
            // nor is affected by its presence.
            let planes = device
                .set_client_capability(u64::from(sys::DRM_CLIENT_CAP_UNIVERSAL_PLANES), 1)
                .is_ok();
            device.atomic = planes
                && device
                    .set_client_capability(u64::from(sys::DRM_CLIENT_CAP_ATOMIC), 1)
                    .is_ok();
        }

        Ok(device)
    }

    /// Opens the first device under `/dev/dri` that can be opened, preferring atomic.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Open`] with why the last candidate refused, or naming the directory when
    /// it holds no `card*` entry at all.
    pub fn open_first() -> Result<Self> {
        Self::open_first_with(Interface::Preferred)
    }

    /// Opens the first device under `/dev/dri` that can be opened, through `interface`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Open`] with why the last candidate refused, or naming the directory when
    /// it holds no `card*` entry at all.
    pub fn open_first_with(interface: Interface) -> Result<Self> {
        let mut cards: Vec<PathBuf> = std::fs::read_dir(DIRECTORY)
            .map_err(|source| Error::Open {
                path: PathBuf::from(DIRECTORY),
                source,
            })?
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("card"))
            })
            .collect();
        cards.sort();

        let mut refused = None;
        for card in &cards {
            match Self::open_with(card, interface) {
                Ok(device) => return Ok(device),
                // The last refusal is the one reported. A machine with one card has one reason,
                // and a machine with several has the reason for the last card tried. Reporting
                // "not found" for a device that is there and answered `EACCES` would name the
                // wrong problem.
                Err(error) => refused = Some(error),
            }
        }

        Err(refused.unwrap_or_else(|| Error::Open {
            path: PathBuf::from(DIRECTORY),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        }))
    }

    /// Returns `true` if this device is driven through the atomic interface.
    ///
    /// Decided once, when the device was opened. Every later call reads this instead of asking the
    /// kernel again.
    pub fn is_atomic(&self) -> bool {
        self.atomic
    }

    /// Returns where this device was opened from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the descriptor, for the modules that issue ioctls against it.
    pub(crate) fn fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }

    /// Returns what the driver reports for `capability`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses the query. A capability it has never heard
    /// of is refused that way.
    pub fn capability(&self, capability: u64) -> Result<u64> {
        let mut request = sys::drm_get_cap {
            capability,
            value: 0,
        };
        ioctl::issue(self.fd(), ioctl::GET_CAP, &mut request)?;
        Ok(request.value)
    }

    /// Returns `true` if dumb buffers can be allocated on this device.
    pub fn supports_dumb_buffers(&self) -> bool {
        self.capability(u64::from(sys::DRM_CAP_DUMB_BUFFER))
            .is_ok_and(|value| value != 0)
    }

    /// Returns `true` if a framebuffer may name a format modifier on this device.
    pub fn supports_format_modifiers(&self) -> bool {
        self.capability(u64::from(sys::DRM_CAP_ADDFB2_MODIFIERS))
            .is_ok_and(|value| value != 0)
    }

    /// Asks the kernel to turn a client capability on.
    fn set_client_capability(&self, capability: u64, value: u64) -> Result<()> {
        let mut request = sys::drm_set_client_cap { capability, value };
        ioctl::issue(self.fd(), ioctl::SET_CLIENT_CAP, &mut request)
    }

    /// Becomes the device's master, which modesetting requires.
    ///
    /// Master is held by the open file description, so dropping the [`Device`] gives it up.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when another process holds it, or when this one lacks the
    /// privilege.
    pub fn become_master(&self) -> Result<()> {
        // `SET_MASTER` is a `DRM_IO` request: the kernel reads and writes nothing through the
        // pointer, and `Request<()>` says so.
        ioctl::issue(self.fd(), ioctl::SET_MASTER, &mut ())
    }

    /// Gives up being the device's master.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the kernel refuses.
    pub fn drop_master(&self) -> Result<()> {
        ioctl::issue(self.fd(), ioctl::DROP_MASTER, &mut ())
    }
}
