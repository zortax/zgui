//! Opening a real device, and building one over a descriptor somebody else opened.
//!
//! Everything here needs a card this user may open, so every test looks for one first. Which card
//! it gets is `support`'s answer, and it is read off the machine — see that module for why the
//! search may not go through this crate.
//!
//! Most of what is asserted is that a call *answers*, rather than what it answers: the answer is a
//! fact about the hardware, and the call working is a fact about this crate. Where a value is held
//! to a shape, that shape is one the kernel guarantees: an object id is never zero, a possible-CRTC
//! mask indexes the resource list, and a connected connector reports a mode with an extent and a
//! rate.

mod support;

use std::path::{Path, PathBuf};

use rustix::fd::OwnedFd;
use rustix::fs::{Mode, OFlags};
use zgui_drm::device::Interface;
use zgui_drm::format::{Format, Modifier};
use zgui_drm::property::ObjectKind;

/// What the kernel numbers `DRM_CAP_DUMB_BUFFER`.
///
/// Named here because `sys` is private: a test reaches this crate the way any other caller does.
const DUMB_BUFFER: u64 = 1;

/// Where the kernel puts display devices.
///
/// Named here because [`zgui_drm::cards`] is what answers this directory's contents. A test that
/// learned what the machine has through that function could not tell a directory holding no card
/// from a `cards` that answers nothing, and would report the second as the first.
const DIRECTORY: &str = "/dev/dri";

/// How long a poll of an idle device is given to answer.
///
/// A poll of a non-blocking descriptor answers in microseconds, and a poll of a blocking one waits
/// for a flip nothing asked for. So this separates the two, and it is long enough that a loaded
/// machine does not report the first as the second. It is a bound rather than an expectation:
/// without it, a test that waited for ever would hang the suite instead of failing it.
const POLL_BOUND: std::time::Duration = std::time::Duration::from_secs(5);

/// Returns the cards under [`DIRECTORY`], sorted, read without [`zgui_drm::cards`].
///
/// Answers nothing where the machine has no card, and says so with the remedy.
fn cards_present(test: &str) -> Option<Vec<PathBuf>> {
    let mut cards: Vec<PathBuf> = std::fs::read_dir(DIRECTORY)
        .map_err(|error| eprintln!("{test}: {DIRECTORY} cannot be read: {error}"))
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("card"))
        })
        .collect();
    cards.sort();

    if cards.is_empty() {
        eprintln!(
            "{test}: {DIRECTORY} holds no card, so nothing was asserted\n\
             load the virtual driver with `sudo modprobe vkms` to run it"
        );
        return None;
    }
    Some(cards)
}

/// Returns a descriptor onto `path`, opened with the flags this crate opens a card with.
///
/// [`zgui_drm::Device::over`] is given a descriptor its caller opened, so the tests open their own
/// rather than reaching into a device this crate built.
fn descriptor(path: &Path) -> OwnedFd {
    rustix::fs::open(
        path,
        OFlags::RDWR | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .unwrap_or_else(|error| panic!("a card that opened once opens again: {error}"))
}

#[test]
fn a_device_opens_and_answers_what_it_can_do() {
    let Some(device) = support::device(
        "a_device_opens_and_answers_what_it_can_do",
        Interface::Preferred,
    ) else {
        return;
    };

    // Asserting that the query *answers* rather than what it answers is the point: the value is a
    // fact about the hardware, and the call working is a fact about this crate.
    assert!(
        device.capability(DUMB_BUFFER).is_ok(),
        "the dumb-buffer capability query is answered"
    );
    println!(
        "{}: atomic={} dumb={} modifiers={}",
        device.path().display(),
        device.is_atomic(),
        device.supports_dumb_buffers(),
        device.supports_format_modifiers(),
    );
}

#[test]
fn a_device_enumerates_its_crtcs_and_connectors() {
    let Some(device) = support::device(
        "a_device_enumerates_its_crtcs_and_connectors",
        Interface::Preferred,
    ) else {
        return;
    };
    let resources = device.resources().expect("the device enumerates");

    // A device that can set a mode has at least one CRTC and at least one connector. A render
    // node would have neither, and `open_first` does not open one — it opens `card*`.
    assert!(
        !resources.crtcs.is_empty(),
        "a modesetting device has at least one CRTC"
    );
    assert!(
        !resources.connectors.is_empty(),
        "a modesetting device has at least one connector"
    );

    for id in &resources.connectors {
        let connector = device
            .connector(*id)
            .expect("a listed connector is readable");
        println!(
            "connector {} {:?} connected={} modes={} preferred={:?}",
            connector.id,
            connector.kind,
            connector.is_connected(),
            connector.modes.len(),
            connector.preferred_mode()
        );
        // A connected connector reports the modes it can be driven at. A disconnected one
        // reports none, and that is the difference this crate models.
        if connector.is_connected() {
            assert!(
                !connector.modes.is_empty(),
                "a connected connector offers at least one mode"
            );
            // A mode a display can actually be driven at has an extent and a rate. Reading them
            // off the hardware checks that the timings were unpacked from the right fields: a
            // mode read out of the wrong offsets produces a zero here.
            let mode = connector
                .preferred_mode()
                .expect("a connector with modes has one to prefer");
            assert!(
                mode.width() != 0,
                "a mode of a connected display has a width"
            );
            assert!(
                mode.height() != 0,
                "a mode of a connected display has a height"
            );
            assert!(
                mode.refresh_rate_millihertz() != 0,
                "a mode of a connected display has a refresh rate"
            );
        }
    }
}

#[test]
fn every_connector_names_encoders_that_reach_a_crtc_in_the_list() {
    let Some(device) = support::device(
        "every_connector_names_encoders_that_reach_a_crtc_in_the_list",
        Interface::Preferred,
    ) else {
        return;
    };
    let resources = device.resources().expect("the device enumerates");
    assert!(
        resources.crtcs.len() < 32,
        "the possible-CRTC mask is a u32"
    );

    for id in &resources.connectors {
        let connector = device
            .connector(*id)
            .expect("a listed connector is readable");
        for encoder in &connector.encoders {
            let encoder = device
                .encoder(*encoder)
                .expect("a named encoder is readable");
            assert!(
                encoder.possible_crtcs < (1_u32 << resources.crtcs.len()),
                "encoder {} indexes the CRTC list",
                encoder.id
            );
            // A connected connector has to be drivable, or nothing could ever show a picture on
            // it. Output discovery rests on this assertion.
            if connector.is_connected() {
                assert_ne!(
                    encoder.possible_crtcs, 0,
                    "an encoder reaching a connected connector drives at least one CRTC"
                );
            }
        }
    }
}

#[test]
fn a_device_enumerates_planes_that_name_the_crtcs_they_can_drive() {
    let test = "a_device_enumerates_planes_that_name_the_crtcs_they_can_drive";
    let Some(device) = support::device(test, Interface::Preferred) else {
        return;
    };
    if !support::atomic(test, &device, "the planes it enumerates") {
        return;
    }

    let planes = device.planes().expect("the device enumerates its planes");
    assert!(
        !planes.is_empty(),
        "an atomic device has at least one plane"
    );

    let resources = device.resources().expect("the device enumerates");
    // A device with 32 or more CRTCs would overflow the mask below, and none exists: the mask is
    // one `u32`, so the kernel cannot describe more than 32 either.
    assert!(
        resources.crtcs.len() < 32,
        "the possible-CRTC mask is a u32, so a device has fewer than 32 CRTCs"
    );

    for plane in &planes {
        let plane = device.plane(*plane).expect("a listed plane is readable");
        assert!(
            !plane.formats.is_empty(),
            "a plane states the formats it can scan out"
        );
        // The mask indexes the CRTC list, so a bit set past its end would mean this crate and
        // the kernel disagree about what the list is.
        assert!(
            plane.possible_crtcs < (1_u32 << resources.crtcs.len()),
            "the possible-CRTC mask indexes the CRTC list"
        );
        println!(
            "plane {} crtcs={:#b} driving={:?} formats={}",
            plane.id,
            plane.possible_crtcs,
            plane.crtc,
            plane.formats.len()
        );
    }
}

#[test]
fn an_atomic_device_names_the_properties_a_commit_is_built_from() {
    let test = "an_atomic_device_names_the_properties_a_commit_is_built_from";
    let Some(device) = support::device(test, Interface::Preferred) else {
        return;
    };
    if !support::atomic(test, &device, "the properties a commit is built from") {
        return;
    }

    let resources = device.resources().expect("the device enumerates");

    // A plane commit sets these. A device missing one cannot be driven by the plane commit this
    // crate builds, and the property list is where that shows.
    let planes = device.planes().expect("the device enumerates its planes");
    let plane = *planes.first().expect("an atomic device has a plane");
    let properties = device
        .properties(plane, ObjectKind::Plane)
        .expect("a plane's properties are readable");
    for name in [
        "FB_ID", "CRTC_ID", "CRTC_X", "CRTC_Y", "CRTC_W", "CRTC_H", "SRC_X", "SRC_Y", "SRC_W",
        "SRC_H",
    ] {
        assert!(
            properties.id(name).is_some(),
            "plane {plane} names its {name} property, and has {:?}",
            properties.names().collect::<Vec<_>>()
        );
    }
    // `type` is an enumeration, and it is how a primary plane is told from a cursor.
    assert!(
        properties.value("type").is_some(),
        "a plane states what kind of plane it is"
    );

    // A CRTC commit sets the mode and turns the CRTC on.
    let crtc = *resources
        .crtcs
        .first()
        .expect("a modesetting device has a CRTC");
    let properties = device
        .properties(crtc, ObjectKind::Crtc)
        .expect("a CRTC's properties are readable");
    for name in ["MODE_ID", "ACTIVE"] {
        assert!(
            properties.id(name).is_some(),
            "CRTC {crtc} names its {name} property, and has {:?}",
            properties.names().collect::<Vec<_>>()
        );
    }

    // A connector commit says which CRTC drives it, and nothing else.
    let connector = *resources
        .connectors
        .first()
        .expect("a modesetting device has a connector");
    let properties = device
        .properties(connector, ObjectKind::Connector)
        .expect("a connector's properties are readable");
    assert!(
        properties.id("CRTC_ID").is_some(),
        "connector {connector} names its CRTC_ID property, and has {:?}",
        properties.names().collect::<Vec<_>>()
    );
}

#[test]
fn a_dumb_buffer_is_allocated_mapped_written_and_released() {
    let Some(device) = support::device(
        "a_dumb_buffer_is_allocated_mapped_written_and_released",
        Interface::Preferred,
    ) else {
        return;
    };
    if !device.supports_dumb_buffers() {
        eprintln!("this device has no dumb buffers, so nothing was asserted");
        return;
    }

    let mut buffer = device
        .create_dumb_buffer(64, 32, Format::XRGB8888)
        .expect("the driver allocates a dumb buffer");
    assert_eq!(buffer.width(), 64);
    assert_eq!(buffer.height(), 32);
    // A driver rounds the row up for its own reasons, so the stride is at least the width in
    // bytes and stepping rows by anything else writes a diagonal.
    assert!(
        buffer.stride() >= 64 * 4,
        "a row holds at least its pixels: {}",
        buffer.stride()
    );

    let stride = buffer.stride() as usize;
    let bytes = buffer.bytes(&device).expect("a dumb buffer maps");
    assert!(bytes.len() >= stride * 32, "the mapping covers every row");
    // Written and read back through the mapping, so the slice addresses memory that takes a write
    // and keeps it. The mapping is shared, at the offset the driver answered for this handle, so
    // that memory is the driver's.
    bytes[..4].copy_from_slice(&0x00ff_0000_u32.to_ne_bytes());
    assert_eq!(bytes[..4], 0x00ff_0000_u32.to_ne_bytes());

    device
        .destroy_dumb_buffer(buffer)
        .expect("a dumb buffer is released");
}

#[test]
fn a_dumb_buffer_is_accepted_for_scanout_and_released() {
    let Some(device) = support::device(
        "a_dumb_buffer_is_accepted_for_scanout_and_released",
        Interface::Preferred,
    ) else {
        return;
    };
    if !device.supports_dumb_buffers() {
        eprintln!("this device has no dumb buffers, so nothing was asserted");
        return;
    }

    let buffer = device
        .create_dumb_buffer(64, 32, Format::XRGB8888)
        .expect("the driver allocates a dumb buffer");
    let framebuffer = device
        .add_framebuffer(&buffer, Format::XRGB8888)
        .expect("the driver accepts a dumb buffer for scanout");
    // Zero is not an object id: the kernel's allocator starts at one, so zero means "no
    // framebuffer" in every commit this crate builds.
    assert_ne!(framebuffer.id(), 0, "an accepted framebuffer has an id");

    device
        .remove_framebuffer(framebuffer)
        .expect("a framebuffer is released");
    device
        .destroy_dumb_buffer(buffer)
        .expect("a dumb buffer is released");
}

#[test]
fn a_framebuffer_states_a_modifier_only_when_it_is_given_one() {
    let Some(device) = support::device(
        "a_framebuffer_states_a_modifier_only_when_it_is_given_one",
        Interface::Preferred,
    ) else {
        return;
    };
    if !device.supports_dumb_buffers() || !device.supports_format_modifiers() {
        eprintln!("this device does not take modifiers, so nothing was asserted");
        return;
    }

    let buffer = device
        .create_dumb_buffer(64, 32, Format::XRGB8888)
        .expect("the driver allocates a dumb buffer");

    // A dumb buffer is row-major, so naming that layout explicitly has to be accepted beside
    // saying nothing about it. `DRM_FORMAT_MOD_LINEAR` is zero, so this is also what proves the
    // flag that turns the modifier array on is raised for it.
    let stated = device
        .add_framebuffer_from_handles(
            buffer.width(),
            buffer.height(),
            Format::XRGB8888,
            [buffer.handle(), 0, 0, 0],
            [buffer.stride(), 0, 0, 0],
            [0; 4],
            Some(Modifier::LINEAR),
        )
        .expect("the driver accepts a linear framebuffer");
    assert_ne!(stated.id(), 0);

    // `Modifier::INVALID` is what a graphics interface reports for a layout it cannot name, and
    // the driver refuses it as a modifier. It has to reach the kernel as no modifier at all.
    let unstated = device
        .add_framebuffer_from_handles(
            buffer.width(),
            buffer.height(),
            Format::XRGB8888,
            [buffer.handle(), 0, 0, 0],
            [buffer.stride(), 0, 0, 0],
            [0; 4],
            Some(Modifier::INVALID),
        )
        .expect("an unnamed layout is accepted as no modifier");
    assert_ne!(unstated.id(), 0);

    device
        .remove_framebuffer(stated)
        .expect("a framebuffer is released");
    device
        .remove_framebuffer(unstated)
        .expect("a framebuffer is released");
    device
        .destroy_dumb_buffer(buffer)
        .expect("a dumb buffer is released");
}

#[test]
fn an_absent_device_is_refused_rather_than_panicking() {
    let error = zgui_drm::Device::open("/dev/dri/card-that-is-not-there")
        .expect_err("a device that is not there cannot be opened");
    assert!(
        matches!(error, zgui_drm::Error::Open { .. }),
        "the refusal names the path rather than the ioctl: {error}"
    );
}

#[test]
fn the_commit_interface_is_the_one_the_device_was_opened_for() {
    let test = "the_commit_interface_is_the_one_the_device_was_opened_for";
    let Some(atomic) = support::device(test, Interface::Preferred) else {
        return;
    };
    if !support::atomic(test, &atomic, "which commit interface a device is given") {
        return;
    }
    assert!(
        zgui_drm::commit::for_device(&atomic).can_test(),
        "an atomic device gets the interface that can validate a configuration first"
    );

    let legacy = zgui_drm::Device::open_with(atomic.path(), Interface::Legacy)
        .expect("the same device opens for the legacy interface");
    assert!(
        !legacy.is_atomic(),
        "asking for the legacy interface has to produce a legacy device"
    );
    assert!(
        !zgui_drm::commit::for_device(&legacy).can_test(),
        "the legacy interface cannot validate a configuration first"
    );
}

#[test]
fn an_idle_device_reports_no_events_rather_than_blocking() {
    let Some(device) = support::device(
        "an_idle_device_reports_no_events_rather_than_blocking",
        Interface::Preferred,
    ) else {
        return;
    };

    // Nothing has been flipped on this descriptor, so the queue is empty and the kernel answers
    // `EAGAIN`. That the call returns at all is the assertion: a descriptor opened blocking would
    // stop here until something else drove the display.
    let events = device
        .poll_events()
        .expect("an empty queue is not a failure");
    assert!(
        events.is_empty(),
        "a device nothing was asked of has nothing to report: {events:?}"
    );
}

#[test]
fn a_device_over_a_descriptor_enumerates_what_the_card_it_names_enumerates() {
    let Some(opened) = support::device(
        "a_device_over_a_descriptor_enumerates_what_the_card_it_names_enumerates",
        Interface::Preferred,
    ) else {
        return;
    };
    let path = opened.path().to_owned();
    let over = zgui_drm::Device::over(descriptor(&path), path.clone())
        .expect("a device is built over an open descriptor");

    let by_open = opened.resources().expect("the opened device enumerates");
    let by_over = over
        .resources()
        .expect("the device over a descriptor enumerates");

    // A card has CRTCs, so an empty comparison is a comparison of nothing: `drm_mode_getresources`
    // answering zero of everything would satisfy every assertion below.
    assert!(
        !by_open.crtcs.is_empty(),
        "a modesetting device has at least one CRTC to compare"
    );

    // The framebuffer list is left out of this comparison because it is the one list that belongs
    // to the descriptor: the kernel answers `count_fbs` from the calling `drm_file`'s own
    // framebuffers, and each open makes a new `drm_file`. Of the other three,
    // `drm_mode_getresources` lists every encoder the card has, and filters the CRTCs and the
    // connectors by the lease the calling `drm_file` holds — with the connectors filtered again by
    // whether that file asked for writeback connectors. Neither descriptor here is a lessee, and
    // this crate asks for the same client capabilities through both, so the three lists agree.
    assert_eq!(
        by_over.crtcs,
        by_open.crtcs,
        "two descriptors onto {} list one set of CRTCs",
        path.display()
    );
    assert_eq!(
        by_over.connectors,
        by_open.connectors,
        "two descriptors onto {} list one set of connectors",
        path.display()
    );
    assert_eq!(
        by_over.encoders,
        by_open.encoders,
        "two descriptors onto {} list one set of encoders",
        path.display()
    );
}

#[test]
fn a_device_over_a_descriptor_carries_the_client_capabilities_this_crate_sets() {
    let test = "a_device_over_a_descriptor_carries_the_client_capabilities_this_crate_sets";
    let Some(opened) = support::device(test, Interface::Preferred) else {
        return;
    };
    let path = opened.path().to_owned();
    let over = zgui_drm::Device::over(descriptor(&path), path.clone())
        .expect("a device is built over an open descriptor");

    // Every assertion above holds on a device that asked the kernel for nothing, because
    // enumeration needs no capability. This one reads whether the capabilities were set, and each
    // descriptor is its own open file description, so the one this crate opened says nothing about
    // the one it was handed.
    //
    // Whether the card has an atomic interface is asked over a descriptor of the support module's
    // own, so the guard reads the kernel rather than the code this test is about. A guard on
    // `over.is_atomic()` would switch the test off exactly when it should fail.
    if !support::atomic(test, &over, "the capabilities a descriptor carries") {
        return;
    }

    // `is_atomic` is this crate's own bookkeeping. The kernel's answer is the property list: the
    // properties a plane commit sets carry `DRM_MODE_PROP_ATOMIC`, and
    // `drm_mode_object_get_properties` hides those from a `drm_file` that never took the atomic
    // capability. So this reads the capability back out of the kernel that recorded it.
    let planes = over
        .planes()
        .expect("an atomic device enumerates its planes");
    let plane = *planes.first().expect("an atomic device has a plane");
    let properties = over
        .properties(plane, ObjectKind::Plane)
        .expect("a plane's properties are readable");
    for name in [
        "FB_ID", "CRTC_ID", "CRTC_X", "CRTC_Y", "CRTC_W", "CRTC_H", "SRC_X", "SRC_Y", "SRC_W",
        "SRC_H",
    ] {
        assert!(
            properties.id(name).is_some(),
            "plane {plane} names its {name} property, which the kernel shows only to a client \
             that set the atomic capability, and has {:?}",
            properties.names().collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_device_over_a_descriptor_is_built_for_the_interface_its_caller_asked_for() {
    let test = "a_device_over_a_descriptor_is_built_for_the_interface_its_caller_asked_for";
    let Some(opened) = support::device(test, Interface::Preferred) else {
        return;
    };
    let path = opened.path().to_owned();

    let preferred = zgui_drm::Device::over(descriptor(&path), path.clone())
        .expect("a device is built over an open descriptor");
    let legacy = zgui_drm::Device::over_with(descriptor(&path), path.clone(), Interface::Legacy)
        .expect("a device is built over an open descriptor");

    // Both devices are read, because the legacy assertion holds on its own for a card that has
    // only the legacy interface. The pair says the argument was honoured. The preferred half is
    // asserted against what the card answers the support module, so a card with no atomic
    // interface reports that here rather than reading as a defect; the legacy half holds on any
    // card and runs either way.
    support::atomic(test, &preferred, "the preferred half of that pair");
    assert!(
        !legacy.is_atomic(),
        "asking for the legacy interface over a descriptor has to produce a legacy device"
    );
    assert!(
        !zgui_drm::commit::for_device(&legacy).can_test(),
        "the legacy interface cannot validate a configuration first"
    );

    // What the kernel recorded, rather than what this crate remembers. `ACTIVE` carries
    // `DRM_MODE_PROP_ATOMIC`, so a `drm_file` that took the atomic capability is the only one
    // shown it.
    let resources = legacy.resources().expect("the device enumerates");
    let crtc = *resources
        .crtcs
        .first()
        .expect("a modesetting device has a CRTC");
    let properties = legacy
        .properties(crtc, ObjectKind::Crtc)
        .expect("a CRTC's properties are readable");
    assert!(
        properties.id("ACTIVE").is_none(),
        "CRTC {crtc} hides its atomic properties from a legacy client, and named {:?}",
        properties.names().collect::<Vec<_>>()
    );
}

#[test]
fn a_device_over_a_blocking_descriptor_polls_rather_than_waiting() {
    let Some(opened) = support::device(
        "a_device_over_a_blocking_descriptor_polls_rather_than_waiting",
        Interface::Preferred,
    ) else {
        return;
    };
    let path = opened.path().to_owned();

    // Opened without `O_NONBLOCK`, which is the descriptor a caller may hand over. The duplicate
    // stays here, so what reached the shared open file description can be read after the fact.
    let blocking = rustix::fs::open(&path, OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())
        .unwrap_or_else(|error| panic!("a card that opened once opens again: {error}"));
    let kept = blocking
        .try_clone()
        .expect("a descriptor onto an open card duplicates");
    let over = zgui_drm::Device::over(blocking, path.clone())
        .expect("a device is built over an open descriptor");

    // The flag lives on the open file description rather than on the descriptor, so the copy the
    // caller kept reports it too. A session daemon's own descriptor is another name for that same
    // description, and this is what it sees.
    let flags = rustix::fs::fcntl_getfl(&kept).expect("a descriptor reports its status flags");
    assert!(
        flags.contains(OFlags::NONBLOCK),
        "the flag reached the description behind the descriptor that was handed over: {flags:?}"
    );

    // The observable behind that flag. Nothing was flipped on this device, so a blocking
    // descriptor waits here for a completion nobody asked for — which at run time is a frame loop
    // stopping dead with nothing printed. The bound makes that a failure rather than a suite that
    // never finishes.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let answer = over
            .poll_events()
            .map(|events| events.len())
            .map_err(|error| error.to_string());
        // The receiver is gone when the bound expired, and there is nobody left to tell.
        let _ = sender.send(answer);
    });
    let answer = receiver.recv_timeout(POLL_BOUND).unwrap_or_else(|_| {
        panic!(
            "polling {} is still waiting after {POLL_BOUND:?}, so the descriptor handed over \
             stayed blocking",
            path.display()
        )
    });

    assert_eq!(
        answer.expect("an empty queue is not a failure"),
        0,
        "a device nothing was asked of has nothing to report"
    );
}

#[test]
fn a_descriptor_that_names_no_drm_device_is_refused_rather_than_built_over() {
    // A session hands out input devices over the interface it hands out cards over, so a
    // descriptor onto something that is not a card is a mistake a caller can make. `/dev/null` is
    // the one every machine has, and it refuses a DRM request number exactly as an evdev
    // descriptor does.
    let other = rustix::fs::open("/dev/null", OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())
        .expect("/dev/null opens");
    let named = PathBuf::from("/dev/dri/card-that-is-not-a-card");
    let error = zgui_drm::Device::over(other, named.clone())
        .expect_err("a descriptor onto something other than a DRM device is refused");
    assert!(
        matches!(error, zgui_drm::Error::Unusable(_)),
        "the refusal says the answer cannot be used: {error:?}"
    );
    assert!(
        error.to_string().contains("card-that-is-not-a-card"),
        "the refusal names the path its caller gave: {error}"
    );

    // The other direction, so that the check above is a check rather than a refusal of everything.
    let Some(opened) = support::device(
        "a_descriptor_that_names_no_drm_device_is_refused_rather_than_built_over",
        Interface::Preferred,
    ) else {
        return;
    };
    let path = opened.path().to_owned();
    zgui_drm::Device::over(descriptor(&path), path.clone())
        .unwrap_or_else(|error| panic!("a descriptor onto {} is taken: {error}", path.display()));
}

#[test]
fn a_device_over_a_descriptor_answers_the_path_its_caller_named() {
    let Some(opened) = support::device(
        "a_device_over_a_descriptor_answers_the_path_its_caller_named",
        Interface::Preferred,
    ) else {
        return;
    };
    let path = opened.path().to_owned();

    // A name nothing can open, over a descriptor that drives a card. The path is carried for
    // messages, and this says so: a call that opened it would be refused here.
    let named = PathBuf::from("/dev/dri/card-the-session-named");
    let over = zgui_drm::Device::over(descriptor(&path), named.clone())
        .expect("a device is built over an open descriptor");

    assert_eq!(
        over.path(),
        named,
        "the device answers the name it was given"
    );
    assert!(
        over.resources().is_ok(),
        "the descriptor drives {}, whatever the device is called",
        path.display()
    );
}

#[test]
fn the_card_list_is_sorted_and_holds_the_card_under_test() {
    let test = "the_card_list_is_sorted_and_holds_the_card_under_test";
    let Some(present) = cards_present(test) else {
        return;
    };

    // `present` is sorted here, so this comparison carries the order as well as the contents. A
    // machine with one card can hold only the contents: one entry is in order whatever `cards`
    // does to it, and a listing whose order can be read needs a second card.
    let cards = zgui_drm::cards().expect("the card list is answered");
    assert_eq!(
        cards, present,
        "the card list holds every card under {DIRECTORY}, in order"
    );

    // The card these tests run against is one the list offers, so a caller that opens its devices
    // through a session daemon reaches the same card by walking it.
    let Some(device) = support::device(test, Interface::Preferred) else {
        return;
    };
    assert!(
        cards.iter().any(|card| card == device.path()),
        "the list holds {}, which is the card under test: {cards:?}",
        device.path().display()
    );
}
