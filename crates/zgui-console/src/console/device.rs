//! The graphics device a display's own buffers are made on.

use std::sync::Arc;

use iceblink_wgpu::{Gpu, SharedGraphics};
use tracing::warn;

/// Opens the device a display's own buffers are made on.
///
/// Answers `None` on a machine with no usable adapter and on one whose driver would not grant the
/// Vulkan device extensions an exported image needs. Both are ordinary facts about a machine rather
/// than a failure to start: the answer to each is the copied path, which every machine has.
///
/// The extensions are read off the device that opened rather than assumed from what was asked for.
/// A device extension can be enabled only while a device is created, so a device without them is
/// the device this program has for the rest of its run.
pub(crate) fn scanout_device(graphics: &SharedGraphics) -> Option<Arc<Gpu>> {
    let gpu = match graphics.open_gpu() {
        Ok(gpu) => gpu,
        Err(failure) => {
            warn!(
                "no graphics device on this machine could compose a frame, so every display copies \
                 its frames through the processor: {failure}"
            );
            return None;
        }
    };
    let enabled = gpu.vulkan_extensions();
    if let Some(missing) = zgui_platform_drm::EXTENSIONS
        .iter()
        .find(|name| !enabled.contains(name))
    {
        warn!(
            "{} did not enable {missing:?}, so every display copies its frames through the \
             processor",
            gpu.describe()
        );
        return None;
    }
    Some(gpu)
}

/// Returns the graphics every display is drawn with, opening its device with the Vulkan device
/// extensions an exported image needs where an adapter has them.
pub(crate) fn graphics() -> SharedGraphics {
    let backends = iceblink_wgpu::wgpu::Backends::VULKAN | iceblink_wgpu::wgpu::Backends::GL;
    SharedGraphics::with_backends_and_extensions(backends, zgui_platform_drm::EXTENSIONS.to_vec())
        .with_options(
            zgui::app::pipeline_directory(),
            Some(zgui::app::latency_marks()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_that_was_asked_for_no_extensions_leaves_every_display_copying() {
        // `SharedGraphics::new()` asks for no Vulkan device extension at all, which is how a
        // machine that grants every one of them reproduces a machine that grants none. A machine
        // with no usable adapter answers the same way for the other reason.
        assert!(
            scanout_device(&SharedGraphics::new()).is_none(),
            "a device that enabled none of them was handed to the frame loop, which would set \
             every display up for images that device cannot make"
        );
    }

    #[test]
    fn a_device_that_grants_them_is_the_one_the_displays_are_built_on() {
        let test = "a_device_that_grants_them_is_the_one_the_displays_are_built_on";
        let Some(gpu) = scanout_device(&graphics()) else {
            eprintln!(
                "{test}: no adapter on this machine grants what an exported image needs, so \
                 nothing was asserted\n\
                 run it on a machine with a Vulkan driver to make it assert anything"
            );
            return;
        };

        for name in zgui_platform_drm::EXTENSIONS {
            assert!(
                gpu.vulkan_extensions().contains(&name),
                "{} was handed to the frame loop without {name:?}",
                gpu.describe()
            );
        }
        eprintln!("{test}: {} carries all of them", gpu.describe());
    }
}
