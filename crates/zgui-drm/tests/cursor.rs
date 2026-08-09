//! What a real device says about its hardware cursor.
//!
//! Everything here reads. A cursor size is a capability, a cursor plane is a plane and a property,
//! and none of the three needs DRM master, so this file asserts under a running compositor.
//! Putting an image on a cursor plane is a modeset, and that lives in `cursor_commit.rs`.

mod support;

use zgui_drm::device::Interface;
use zgui_drm::property::ObjectKind;

/// What the kernel numbers `DRM_PLANE_TYPE_CURSOR`.
///
/// Named here because `sys` is private, and because no vendored header declares it: the values of
/// the `type` property are the kernel's `enum drm_plane_type`. A unit test inside the crate checks
/// this number against the name the kernel puts beside it.
const CURSOR: u64 = 2;

#[test]
fn a_device_states_a_cursor_size_a_caller_can_allocate() {
    let Some(device) = support::device(
        "a_device_states_a_cursor_size_a_caller_can_allocate",
        Interface::Preferred,
    ) else {
        return;
    };

    let size = device.cursor_size();
    println!(
        "{}: cursor {}x{}",
        device.path().display(),
        size.width,
        size.height
    );

    // The query is answered either way: a driver that refuses it wants the historical 64x64. A
    // zero here would be a size nothing can allocate, and it is what a defaulting that reported
    // the driver's own answer unchanged would produce.
    assert_ne!(size.width, 0, "a cursor buffer has a width to allocate");
    assert_ne!(size.height, 0, "a cursor buffer has a height to allocate");
}

#[test]
fn a_cursor_plane_is_one_that_says_it_is_and_can_drive_the_crtc_it_was_asked_for() {
    let Some(device) = support::device(
        "a_cursor_plane_is_one_that_says_it_is_and_can_drive_the_crtc_it_was_asked_for",
        Interface::Preferred,
    ) else {
        return;
    };
    if !device.is_atomic() {
        eprintln!("this device has no universal planes, so it lists no cursor plane");
        return;
    }

    let resources = device.resources().expect("the device enumerates");
    let mut found = 0;
    for (index, crtc) in resources.crtcs.iter().enumerate() {
        let Some(id) = device
            .cursor_plane(index)
            .expect("the device answers what cursor plane it has")
        else {
            println!("CRTC {crtc} has no cursor plane");
            continue;
        };
        found += 1;

        let plane = device.plane(id).expect("a named cursor plane is readable");
        assert!(
            plane.drives(index),
            "cursor plane {id} was picked for CRTC {crtc}, so its mask covers that place in the \
             list: {:#b}",
            plane.possible_crtcs
        );
        assert_eq!(
            device
                .properties(id, ObjectKind::Plane)
                .expect("a plane's properties are readable")
                .value("type"),
            Some(CURSOR),
            "the plane picked for CRTC {crtc} states that it is a cursor plane"
        );
        println!("CRTC {crtc} has cursor plane {id}");
    }

    // A device where every CRTC answered `None` is one a caller composites a pointer on, and this
    // machine may be it. Saying which happened is the honest outcome.
    if found == 0 {
        eprintln!("no CRTC on this device offers a cursor plane, so only the absence was asserted");
    }
}

#[test]
fn a_cursor_plane_drives_only_the_crtcs_its_mask_names() {
    let Some(device) = support::device(
        "a_cursor_plane_drives_only_the_crtcs_its_mask_names",
        Interface::Preferred,
    ) else {
        return;
    };
    if !device.is_atomic() {
        eprintln!("this device has no universal planes, so it lists no cursor plane");
        return;
    }

    let resources = device.resources().expect("the device enumerates");
    // A plane picked for a CRTC it cannot drive is the defect this guards: the mask indexes the
    // CRTC list, so reading it with a CRTC *id* picks a plane at random on any device whose ids
    // are not 0, 1, 2. They never are.
    for (index, crtc) in resources.crtcs.iter().enumerate() {
        let Some(id) = device
            .cursor_plane(index)
            .expect("the device answers what cursor plane it has")
        else {
            continue;
        };
        let mask = device
            .plane(id)
            .expect("a named cursor plane is readable")
            .possible_crtcs;
        assert_ne!(
            mask & (1 << index),
            0,
            "cursor plane {id} for CRTC {crtc} names place {index} in the CRTC list"
        );
    }
}

#[test]
fn a_cursor_plane_names_every_property_an_atomic_cursor_commit_sets() {
    let Some(device) = support::device(
        "a_cursor_plane_names_every_property_an_atomic_cursor_commit_sets",
        Interface::Preferred,
    ) else {
        return;
    };
    if !device.is_atomic() {
        eprintln!("this device is not atomic, so it has no properties to name");
        return;
    }

    let resources = device.resources().expect("the device enumerates");
    let Some(id) = resources
        .crtcs
        .iter()
        .enumerate()
        .find_map(|(index, _)| device.cursor_plane(index).ok().flatten())
    else {
        eprintln!("this device offers no cursor plane, so nothing was asserted");
        return;
    };

    let properties = device
        .properties(id, ObjectKind::Plane)
        .expect("a plane's properties are readable");
    // The same ten a primary plane is driven by. A cursor plane missing one cannot be driven
    // atomically, and the property list is where that shows.
    for name in [
        "FB_ID", "CRTC_ID", "CRTC_X", "CRTC_Y", "CRTC_W", "CRTC_H", "SRC_X", "SRC_Y", "SRC_W",
        "SRC_H",
    ] {
        assert!(
            properties.id(name).is_some(),
            "cursor plane {id} names its {name} property, and has {:?}",
            properties.names().collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_device_opened_for_the_legacy_interface_lists_no_cursor_plane() {
    let Some(device) = support::device(
        "a_device_opened_for_the_legacy_interface_lists_no_cursor_plane",
        Interface::Legacy,
    ) else {
        return;
    };
    assert!(
        !device.is_atomic(),
        "a device opened for the legacy interface drives the legacy path"
    );

    // The kernel hides primary and cursor planes from a client that did not ask for universal
    // planes. So the answer is nothing, and the legacy cursor request names the CRTC instead.
    let resources = device.resources().expect("the device enumerates");
    for index in 0..resources.crtcs.len() {
        assert_eq!(
            device
                .cursor_plane(index)
                .expect("the device answers what cursor plane it has"),
            None,
            "a legacy client is shown no cursor plane for the CRTC at place {index}"
        );
    }
}
