//! Finding a device to test against, and saying so when there is none.
//!
//! `cargo xtask ledger ignored` forbids switching a test off, and states the alternative: a test
//! that needs a device looks for one, reports on standard error that it did not find one, and
//! returns. That is what this is. The refusal is then a fact about the machine, printed where it
//! happened, rather than a permanent property of the source.
//!
//! A message that names no remedy is the same as no message, because a third of this crate's
//! coverage is behind one. So every refusal below says what to do about it.

// This module is compiled into each integration test binary, and each one uses the part of it that
// its own subject needs. So a helper is dead code in the binaries that do not call it, which says
// nothing about the workspace.
#![allow(dead_code)]

use std::path::PathBuf;

use zgui_drm::Device;
use zgui_drm::device::Interface;
use zgui_drm::property::ObjectKind;

/// The environment variable that names one device to test against.
///
/// [`Device::open_first_with`] sorts `/dev/dri/card*` and opens the first that answers, which is
/// the right default and the wrong thing when a machine has two. The tests that set a mode need
/// DRM master, a compositor holds master on the card it is running on, and `sudo modprobe vkms`
/// adds a card nobody holds — with a minor number that may sort above the held one. This is how
/// that card is named rather than hoped for.
pub(crate) const DEVICE: &str = "ZGUI_DRM_DEVICE";

/// What the kernel numbers `DRM_PLANE_TYPE_PRIMARY`.
///
/// Named here because `sys` is private: a test reaches this crate the way any other caller does.
const PRIMARY: u64 = 1;

/// Returns a device to test against, or nothing.
///
/// `interface` makes the legacy half of this crate testable: asking for [`Interface::Legacy`] on
/// an atomic device gets a device that serves the legacy ioctls, because the kernel implements
/// them over its own atomic helpers.
pub(crate) fn device(test: &str, interface: Interface) -> Option<Device> {
    let named = std::env::var_os(DEVICE).map(PathBuf::from);
    let opened = match &named {
        Some(path) => Device::open_with(path, interface),
        None => Device::open_first_with(interface),
    };

    match opened {
        Ok(device) => Some(device),
        Err(error) => {
            match &named {
                Some(path) => eprintln!(
                    "{test}: {DEVICE} names {}, which does not open, so nothing was asserted: \
                     {error}",
                    path.display()
                ),
                None => eprintln!(
                    "{test}: no DRM device on this machine, so nothing was asserted: {error}\n\
                     load the virtual driver with `sudo modprobe vkms` to run it"
                ),
            }
            None
        }
    }
}

/// Takes DRM master, and reports what stands in the way and how to get past it.
///
/// Returns `true` if the caller may go on. Modesetting needs master, one open device has one
/// master, and `drm_setmaster_ioctl` checks the caller's privilege and *then* refuses a device
/// that already has one. So running as root under a compositor turns the refusal from `EACCES`
/// into `EBUSY` and changes nothing else, and the remedy below names a second device rather than a
/// second privilege.
pub(crate) fn master(test: &str, device: &Device) -> bool {
    match device.become_master() {
        Ok(()) => true,
        Err(error) => {
            eprintln!(
                "{test}: cannot take DRM master on {}, so nothing was asserted: {error}\n\
                 a compositor holds master for as long as it runs, and running as root does not \
                 take it away: one device has one master.\n\
                 run this from a free virtual terminal, or add a device nobody holds with `sudo \
                 modprobe vkms` and name it with {DEVICE}=/dev/dri/cardN",
                device.path().display()
            );
            false
        }
    }
}

/// Returns the primary plane that can drive the CRTC at `crtc_index`, where the device has one.
///
/// Two things make a plane the right one. Its possible-CRTC mask indexes the resource list, so the
/// index selects the bit. Its `type` states what it is: a cursor or an overlay plane takes the same
/// commit and puts no mode on the screen.
pub(crate) fn primary_plane(device: &Device, crtc_index: usize) -> Option<u32> {
    device.planes().ok()?.into_iter().find(|id| {
        let Ok(plane) = device.plane(*id) else {
            return false;
        };
        plane.drives(crtc_index)
            && device
                .properties(*id, ObjectKind::Plane)
                .ok()
                .and_then(|properties| properties.value("type"))
                == Some(PRIMARY)
    })
}
