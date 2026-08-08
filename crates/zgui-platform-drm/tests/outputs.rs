//! Discovering the displays on a real device.

use std::collections::HashSet;

use zgui_drm::Device;
use zgui_drm::device::Interface;
use zgui_platform_drm::Output;

/// Returns a device to test against, or nothing.
///
/// `cargo xtask ledger ignored` forbids switching a test off, and states the alternative: a test
/// that needs a device looks for one, reports on standard error that it did not find one, and
/// returns. The refusal is then a fact about the machine, printed where it happened, rather than a
/// permanent property of the source.
fn device(test: &str) -> Option<Device> {
    match Device::open_first_with(Interface::Preferred) {
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

#[test]
fn every_display_that_is_plugged_in_gets_a_crtc_and_a_plane() {
    let Some(device) = device("every_display_that_is_plugged_in_gets_a_crtc_and_a_plane") else {
        return;
    };
    let outputs = Output::discover(&device).expect("the device is readable");

    let resources = device.resources().expect("the device enumerates");
    let connected = resources
        .connectors
        .iter()
        .filter(|id| {
            let connector = device
                .connector(**id)
                .expect("a listed connector is readable");
            connector.is_connected() && connector.preferred_mode().is_some()
        })
        .count();
    assert_eq!(
        outputs.len(),
        connected,
        "every display that is plugged in is driven, on a device with a CRTC to spare for each"
    );

    let mut crtcs = HashSet::new();
    let mut planes = HashSet::new();
    for output in &outputs {
        println!(
            "connector {} crtc {} plane {} {}x{} @ {}mHz",
            output.pipe.connector,
            output.pipe.crtc,
            output.pipe.plane,
            output.mode.width(),
            output.mode.height(),
            output.mode.refresh_rate_millihertz(),
        );

        // Two CRTCs scanning out the same picture is a configuration; one CRTC driving two
        // displays is a bug that shows up as one of them staying black.
        assert!(
            crtcs.insert(output.pipe.crtc),
            "no two outputs share a CRTC"
        );
        // A mode read out of the wrong fields is a zero here, and a zero-sized display is a
        // division by zero somewhere downstream.
        assert!(output.mode.width() != 0, "the mode has a width");
        assert!(output.mode.height() != 0, "the mode has a height");

        if device.is_atomic() {
            assert!(
                planes.insert(output.pipe.plane),
                "no two outputs scan out from the same plane"
            );
        } else {
            assert_eq!(
                output.pipe.plane, 0,
                "a legacy device has no plane object to name"
            );
        }
    }
}
