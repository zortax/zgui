//! Finding a device to test against, and saying so when there is none.
//!
//! `cargo xtask ledger ignored` forbids switching a test off, and states the alternative: a test
//! that needs a device looks for one, reports on standard error that it did not find one, and
//! returns. That is what this is. The refusal is then a fact about the machine, printed where it
//! happened, rather than a permanent property of the source.

use zgui_drm::Device;
use zgui_drm::device::Interface;

/// Returns a device to test against, or nothing.
///
/// `interface` makes the legacy half of this crate testable: asking for [`Interface::Legacy`] on
/// an atomic device gets a device that serves the legacy ioctls, because the kernel implements
/// them over its own atomic helpers.
pub(crate) fn device(test: &str, interface: Interface) -> Option<Device> {
    match Device::open_first_with(interface) {
        Ok(device) => Some(device),
        Err(error) => {
            eprintln!(
                "{test}: no DRM device on this machine, so nothing was asserted: {error}\n\
                 load the virtual driver with `sudo modprobe vkms` to run it"
            );
            None
        }
    }
}
