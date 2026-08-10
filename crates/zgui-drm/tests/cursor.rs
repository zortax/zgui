//! What a real device says about its hardware cursor.
//!
//! Everything here reads. A cursor size is a capability, a cursor plane is a plane and a property,
//! and none of the three needs DRM master, so this file asserts under a running compositor.
//! Putting an image on a cursor plane is a modeset, and that lives in `cursor_commit.rs`.

mod support;

use zgui_drm::device::Interface;
use zgui_drm::property::ObjectKind;

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
    let test = "a_cursor_plane_is_one_that_says_it_is_and_can_drive_the_crtc_it_was_asked_for";
    let Some(device) = support::device(test, Interface::Preferred) else {
        return;
    };
    if !support::atomic(test, &device, "the cursor plane it hands out") {
        return;
    }

    let resources = device.resources().expect("the device enumerates");
    let mut found = 0;
    for (index, crtc) in resources.crtcs.iter().enumerate() {
        let Some(id) = device
            .cursor_plane(index, &[])
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
        // A selection that ignored `type` would answer the primary plane, so the answer here has
        // to be a different one. Which value means cursor is checked inside the crate, against the
        // name the kernel puts beside it, so it is not transcribed a second time here.
        assert_ne!(
            Some(id),
            support::primary_plane(&device, index),
            "the plane picked for CRTC {crtc} is a cursor plane rather than its primary one"
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
    let test = "a_cursor_plane_drives_only_the_crtcs_its_mask_names";
    let Some(device) = support::device(test, Interface::Preferred) else {
        return;
    };
    if !support::atomic(test, &device, "the CRTCs a cursor plane names") {
        return;
    }

    let resources = device.resources().expect("the device enumerates");
    // A plane picked for a CRTC it cannot drive is the defect this guards: the mask indexes the
    // CRTC list, so reading it with a CRTC *id* picks a plane at random on any device whose ids
    // are not 0, 1, 2. They never are.
    for (index, crtc) in resources.crtcs.iter().enumerate() {
        let Some(id) = device
            .cursor_plane(index, &[])
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
fn a_cursor_plane_already_taken_is_never_handed_out_a_second_time() {
    let test = "a_cursor_plane_already_taken_is_never_handed_out_a_second_time";
    let Some(device) = support::device(test, Interface::Preferred) else {
        return;
    };
    if !support::atomic(test, &device, "which cursor plane is handed out twice") {
        return;
    }

    let resources = device.resources().expect("the device enumerates");
    // What a caller driving several displays does: assign in order, and hand in what is gone. A
    // plane whose mask names two CRTCs drives one of them at a time, so handing it out twice would
    // take the first display's cursor away when the second one set its own.
    let mut taken: Vec<u32> = Vec::new();
    for index in 0..resources.crtcs.len() {
        let Some(id) = device
            .cursor_plane(index, &taken)
            .expect("the device answers what cursor plane it has")
        else {
            continue;
        };
        assert!(
            !taken.contains(&id),
            "cursor plane {id} was handed out twice, and one plane shows one cursor"
        );
        taken.push(id);
    }
    println!("cursor planes assigned in order: {taken:?}");

    // And the same plane asked for twice for the same CRTC answers once.
    if let Some(first) = taken.first() {
        let index = (0..resources.crtcs.len())
            .find(|index| {
                device
                    .cursor_plane(*index, &[])
                    .is_ok_and(|found| found == Some(*first))
            })
            .expect("the plane was found for some CRTC a moment ago");
        assert_ne!(
            device
                .cursor_plane(index, &[*first])
                .expect("the device answers what cursor plane it has"),
            Some(*first),
            "a plane named as taken is never the answer"
        );
    }
}

#[test]
fn a_cursor_plane_names_every_property_an_atomic_cursor_commit_sets() {
    let test = "a_cursor_plane_names_every_property_an_atomic_cursor_commit_sets";
    let Some(device) = support::device(test, Interface::Preferred) else {
        return;
    };
    if !support::atomic(test, &device, "the properties an atomic cursor commit sets") {
        return;
    }

    let resources = device.resources().expect("the device enumerates");
    let Some(id) = resources
        .crtcs
        .iter()
        .enumerate()
        .find_map(|(index, _)| device.cursor_plane(index, &[]).ok().flatten())
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
                .cursor_plane(index, &[])
                .expect("the device answers what cursor plane it has"),
            None,
            "a legacy client is shown no cursor plane for the CRTC at place {index}"
        );
    }
}
