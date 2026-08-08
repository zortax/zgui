//! Discovering the displays on a real device, and the handles their surfaces report.

use std::collections::HashSet;
use std::os::fd::{AsFd, AsRawFd};
use std::sync::Arc;

use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use zgui_drm::Device;
use zgui_drm::device::Interface;
use zgui_platform::{Surface, SurfaceId};
use zgui_platform_drm::{DrmSurface, Output};

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

#[test]
fn a_surface_reports_the_device_and_the_plane_it_draws_through() {
    let Some(device) = device("a_surface_reports_the_device_and_the_plane_it_draws_through") else {
        return;
    };
    let outputs = Output::discover(&device).expect("the device is readable");
    // Read before the device is shared, so what the surface reports is compared against the
    // descriptor this test opened rather than against itself.
    let opened = device.as_fd().as_raw_fd();
    let device = Arc::new(device);

    for (number, output) in (1..).zip(&outputs) {
        let plane = output.pipe.plane;
        let surface = Arc::new(DrmSurface::new(
            SurfaceId::new(number),
            output.clone(),
            Arc::clone(&device),
        ));

        assert!(
            surface.gpu().is_some(),
            "a display has native handles, so the surface over it reports some"
        );
        let handles = Arc::clone(&surface)
            .gpu_shared()
            .expect("a surface that answers `gpu` answers `gpu_shared`");

        // The variant is asserted as well as the value: a handle of some other kind would satisfy
        // an equality on a number it never carried.
        let reported_fd = match handles
            .display_handle()
            .expect("the display handle is built")
            .as_raw()
        {
            RawDisplayHandle::Drm(handle) => handle.fd,
            other => panic!("a KMS display reports a DRM display handle, and this is {other:?}"),
        };
        assert_eq!(
            reported_fd, opened,
            "the display handle carries the descriptor of the device the surface holds"
        );

        let reported_plane = match handles
            .window_handle()
            .expect("the window handle is built")
            .as_raw()
        {
            RawWindowHandle::Drm(handle) => handle.plane,
            other => panic!("a KMS display reports a DRM window handle, and this is {other:?}"),
        };
        assert_eq!(
            reported_plane, plane,
            "the window handle carries the plane this output scans out from"
        );

        println!("surface {number} reports fd {reported_fd} and plane {reported_plane}");
    }
}
