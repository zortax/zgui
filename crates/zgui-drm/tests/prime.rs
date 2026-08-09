//! Carrying a buffer out of this device as a descriptor, and back in as a handle.
//!
//! This is the half of graphics interoperation that uses no graphics API. A dumb buffer stands in
//! for the image a Vulkan device would export, so the round trip is exercised against a real
//! kernel with nothing above this crate.
//!
//! # What this needs to assert anything
//!
//! A device that opens. Exporting and importing take no DRM master, so this runs under a
//! compositor and on any card the user may open.

mod support;

use std::os::fd::AsFd;

use zgui_drm::device::Interface;
use zgui_drm::format::{Format, Modifier};

/// What the kernel numbers `DRM_CAP_PRIME`.
///
/// Named here because `sys` is private: a test reaches this crate the way any other caller does.
const PRIME: u64 = 5;

/// `DRM_PRIME_CAP_IMPORT | DRM_PRIME_CAP_EXPORT`, the two bits `DRM_CAP_PRIME` reports.
///
/// Every driver sets both from kernel 6.6 onwards, so reading them makes an older kernel say so on
/// standard error.
const BOTH_DIRECTIONS: u64 = 3;

/// How wide the buffer under test is.
const WIDTH: u32 = 64;

/// How tall the buffer under test is.
const HEIGHT: u32 = 32;

#[test]
fn an_exported_buffer_imports_back_and_is_accepted_for_scanout() {
    let test = "an_exported_buffer_imports_back_and_is_accepted_for_scanout";
    let Some(device) = support::device(test, Interface::Preferred) else {
        return;
    };
    if !can_share(test, &device) {
        return;
    }

    let buffer = device
        .create_dumb_buffer(WIDTH, HEIGHT, Format::XRGB8888)
        .expect("the driver allocates a dumb buffer");
    let allocated = buffer.handle();
    let stride = buffer.stride();

    let descriptor = device
        .export_buffer(&buffer)
        .expect("the driver exports a dumb buffer as a descriptor");
    let imported = device
        .import_buffer(descriptor.as_fd())
        .expect("the driver imports the descriptor it just handed out");

    // The kernel answers with a handle this open descriptor already has for the memory, so a round
    // trip on one device names the buffer it started from. This is the property a caller must not
    // assume away: one memory object has one handle here, and that handle owes one release.
    assert_eq!(
        imported.handle(),
        allocated,
        "a descriptor exported from this device imports back as the handle it was exported from"
    );

    // An imported handle is what a graphics API's image arrives as, and this is the call the
    // platform backend builds its scanout framebuffer with. `Modifier::LINEAR` is the layout a
    // dumb buffer has, and a handle that named no object on this device would be refused here.
    let framebuffer = device
        .add_framebuffer_from_handles(
            WIDTH,
            HEIGHT,
            Format::XRGB8888,
            [imported.handle(), 0, 0, 0],
            [stride, 0, 0, 0],
            [0; 4],
            Some(Modifier::LINEAR),
        )
        .expect("the driver accepts an imported handle for scanout");
    // Zero is not an object id: the kernel's allocator starts at one, so zero means "no
    // framebuffer" in every commit this crate builds.
    assert_ne!(framebuffer.id(), 0, "an accepted framebuffer has an id");

    device
        .remove_framebuffer(framebuffer)
        .expect("a framebuffer is released");
    // The descriptor goes first, to show that the handle outlives it: the kernel holds its own
    // reference on the memory from the moment the import returned.
    drop(descriptor);
    // One handle, one release. The import and the dumb buffer name the same handle, so this is the
    // release both of them owe, and `destroy_dumb_buffer` would be a second one the driver answers
    // `EINVAL` to. `buffer` then goes out of scope holding a name that is already gone.
    device
        .release_imported(imported)
        .expect("an imported handle is released");
}

#[test]
fn one_descriptor_imported_twice_names_one_handle() {
    let test = "one_descriptor_imported_twice_names_one_handle";
    let Some(device) = support::device(test, Interface::Preferred) else {
        return;
    };
    if !can_share(test, &device) {
        return;
    }

    let buffer = device
        .create_dumb_buffer(WIDTH, HEIGHT, Format::XRGB8888)
        .expect("the driver allocates a dumb buffer");
    let descriptor = device
        .export_buffer(&buffer)
        .expect("the driver exports a dumb buffer as a descriptor");

    let first = device
        .import_buffer(descriptor.as_fd())
        .expect("the driver imports a descriptor");
    let second = device
        .import_buffer(descriptor.as_fd())
        .expect("the driver imports the same descriptor again");

    // This is what `Device::import_buffer` documents, checked against the kernel it is a claim
    // about. A caller that released both of these would close the buffer once and then close
    // whatever the driver allocated next under the same number.
    assert_eq!(
        first.handle(),
        second.handle(),
        "two imports of one descriptor name one handle"
    );

    drop(descriptor);
    // One handle, one release. The second import named the handle the first one holds, and the
    // dumb buffer was allocated under it, so both of those go out of scope owing nothing.
    device
        .release_imported(first)
        .expect("an imported handle is released");
}

/// Returns `true` if `device` can carry a buffer over a descriptor, and reports on standard error
/// where it cannot.
fn can_share(test: &str, device: &zgui_drm::Device) -> bool {
    if !device.supports_dumb_buffers() {
        eprintln!(
            "{test}: this device has no dumb buffers, so nothing was asserted\n\
             add a device that has them with `sudo modprobe vkms` and name it with {}=/dev/dri/cardN",
            support::DEVICE
        );
        return false;
    }

    let prime = device.capability(PRIME).unwrap_or(0);
    if prime & BOTH_DIRECTIONS != BOTH_DIRECTIONS {
        eprintln!(
            "{test}: this driver shares buffers in {prime:#x} of the two directions, so nothing \
             was asserted\n\
             kernel 6.6 and later advertise both on every driver: upgrade the kernel, or name a \
             driver that does with {}=/dev/dri/cardN",
            support::DEVICE
        );
        return false;
    }

    true
}
