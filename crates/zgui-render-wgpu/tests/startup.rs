//! What opening a device costs before it has opened one.
//!
//! These assertions are counts rather than durations, because the thing being protected is work
//! that is not done: a backend kept as a fallback must not be interrogated on a machine whose
//! primary backend works. They live in a binary of their own because the count is per process and
//! a test that asks for a fallback on purpose would otherwise raise it.

use zgui_geom::{Scale, Size};
use zgui_render::RenderTarget;
use zgui_render_wgpu::gpu::adapter;
use zgui_render_wgpu::{Builder, wgpu};

/// The extent the device is opened at; nothing here draws.
fn target() -> RenderTarget {
    RenderTarget::new(Size::new(64, 64), Scale::new(1.0))
}

#[test]
fn no_gl_adapter_is_enumerated_when_a_native_device_opens() {
    assert_eq!(
        adapter::gl_enumerations(),
        0,
        "nothing has asked for an adapter yet"
    );

    let built = Builder::new().offscreen(target(), wgpu::TextureFormat::Bgra8Unorm, false);

    match built {
        Ok(renderer) => {
            let backend = renderer.gpu().adapter().get_info().backend;
            if backend != wgpu::Backend::Gl {
                assert_eq!(
                    adapter::gl_enumerations(),
                    0,
                    "a native device opened, so GL's adapters were never asked for"
                );
            } else {
                // The complement, on a machine whose primary backend produced nothing: the
                // fallback tier was enumerated and it is what opened the device.
                assert!(
                    adapter::gl_enumerations() > 0,
                    "a GL device can only have come from an enumerated fallback"
                );
                eprintln!("no native device here; the fallback opened {backend:?}");
            }
        }
        Err(failure) => {
            // No device at all: every tier was tried, which is the only case in which the
            // fallback is enumerated without a device coming out of it. A platform whose
            // default set carries no GL has no fallback to enumerate.
            if adapter::default_backends().intersects(wgpu::Backends::GL) {
                assert!(
                    adapter::gl_enumerations() > 0,
                    "the fallback is enumerated before the attempt is given up"
                );
            } else {
                assert_eq!(
                    adapter::gl_enumerations(),
                    0,
                    "this platform's default set has no GL to enumerate"
                );
            }
            eprintln!("skipped: no usable graphics device ({failure})");
        }
    }
}
