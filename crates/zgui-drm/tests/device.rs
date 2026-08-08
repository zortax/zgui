//! Opening a real device.

mod support;

use zgui_drm::device::Interface;

/// What the kernel numbers `DRM_CAP_DUMB_BUFFER`.
///
/// Named here because `sys` is private: a test reaches this crate the way any other caller does.
const DUMB_BUFFER: u64 = 1;

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
fn an_absent_device_is_refused_rather_than_panicking() {
    let error = zgui_drm::Device::open("/dev/dri/card-that-is-not-there")
        .expect_err("a device that is not there cannot be opened");
    assert!(
        matches!(error, zgui_drm::Error::Open { .. }),
        "the refusal names the path rather than the ioctl: {error}"
    );
}
