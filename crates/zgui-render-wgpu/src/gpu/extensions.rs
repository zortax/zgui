//! Enabling Vulkan device extensions that wgpu does not ask for on its own.
//!
//! A Vulkan image created with a DRM format modifier and exported as a dma-buf needs device
//! extensions, and a device extension can be enabled only while the device is created. The names
//! go on through wgpu's hal: an adapter can be opened with a callback that adds to the list
//! `vkCreateDevice` is given.
//!
//! Only a target whose wgpu links a Vulkan backend has such an adapter. The build script states
//! that as `vulkan_hal`. Everywhere else the two entry points below answer that no adapter grants
//! anything and that no device was opened. A GL adapter is answered the same way, so no caller
//! needs a second path.

use std::ffi::CStr;

#[cfg(vulkan_hal)]
pub(crate) use present::{granted_by, open};

#[cfg(not(vulkan_hal))]
pub(crate) use absent::{granted_by, open};

/// A device opened with extensions: the device, its queue, and the names enabled on it.
pub(crate) type Opened = (wgpu::Device, wgpu::Queue, Vec<&'static CStr>);

/// The real thing, on the targets whose wgpu links a Vulkan backend.
#[cfg(vulkan_hal)]
mod present {
    use std::ffi::CStr;

    use crate::gpu::adapter;
    use crate::gpu::extensions::Opened;

    /// Returns `true` where a device made from `adapter` would enable every one of `extensions`.
    ///
    /// Asked before any device exists, so the candidate loop can prefer an adapter that grants the
    /// list without opening a device to find out. Answers `false` for an adapter that is not a
    /// Vulkan one, since no other backend has Vulkan device extensions, and `true` for the empty
    /// list, which every adapter grants.
    pub(crate) fn granted_by(adapter: &wgpu::Adapter, extensions: &[&'static CStr]) -> bool {
        if extensions.is_empty() {
            return true;
        }
        // SAFETY: `as_hal` asks that the resource behind the guard is not destroyed. Nothing here
        // destroys anything — the guard is read through and dropped, which its own documentation
        // permits at any time.
        let Some(hal) = (unsafe { adapter.as_hal::<wgpu::hal::api::Vulkan>() }) else {
            return false;
        };
        let physical = hal.physical_device_capabilities();
        extensions
            .iter()
            .all(|name| physical.supports_extension(name))
    }

    /// Opens a device with every one of `extensions` enabled, and answers the names it carries.
    ///
    /// `None` is a report, and every one of its causes is ordinary: an adapter that is not Vulkan,
    /// a name the physical device lacks, a driver that refuses the device. The caller opens the
    /// device the ordinary way instead.
    ///
    /// The list is all or nothing. A physical device that lacks any one of the names is passed
    /// over before a device is created, so a caller is never handed a partly enabled set.
    ///
    /// The names answered are the names asked for. wgpu-hal appends them to the list
    /// `vkCreateDevice` is given, so a device that was created is a device that has them.
    pub(crate) fn open(
        adapter: &wgpu::Adapter,
        extensions: &[&'static CStr],
        descriptor: &wgpu::DeviceDescriptor<'_>,
    ) -> Option<Opened> {
        if extensions.is_empty() {
            return None;
        }
        let named = || adapter::describe(&adapter.get_info());
        // SAFETY: `as_hal` asks that the resource behind the guard is not destroyed. Nothing here
        // destroys anything — the guard is read through and dropped, which its own documentation
        // permits at any time.
        let hal = unsafe { adapter.as_hal::<wgpu::hal::api::Vulkan>() }?;
        let physical = hal.physical_device_capabilities();
        // `vkCreateDevice` answers `VK_ERROR_EXTENSION_NOT_PRESENT` for a name the physical device
        // does not have, and wgpu-hal maps that result to its `hal_usage_error`, which panics. A
        // missing name is therefore found here, before the driver is asked.
        if let Some(missing) = extensions
            .iter()
            .find(|name| !physical.supports_extension(name))
        {
            tracing::debug!(
                adapter = %named(),
                extension = ?missing,
                "the physical device does not have a Vulkan device extension that was asked for"
            );
            return None;
        }

        let opened = match create(&hal, extensions, descriptor) {
            Ok(opened) => opened,
            Err(error) => {
                tracing::warn!(
                    adapter = %named(),
                    %error,
                    "a device with the Vulkan device extensions asked for was refused"
                );
                return None;
            }
        };
        // The names asked for. `Device::enabled_device_extensions` would answer wgpu-hal's own
        // record of the list it handed `vkCreateDevice`, which the callback in `create` built from
        // this same slice — so comparing the two can only ever agree, and Vulkan offers no call
        // that reads back which device extensions a device actually enabled. What the driver
        // refusing a name looks like is `create_device` failing, which is handled above.
        let enabled = extensions.to_vec();

        // SAFETY: `opened` was created from this adapter's own hal adapter, immediately above, and
        // from `descriptor`, so `descriptor.required_features` is the exact feature set it has.
        match unsafe { adapter.create_device_from_hal(opened, descriptor) } {
            Ok((device, queue)) => Some((device, queue, enabled)),
            Err(error) => {
                tracing::warn!(
                    adapter = %named(),
                    %error,
                    "a device created through the hal could not be adopted"
                );
                None
            }
        }
    }

    /// Creates the device, with the features, limits and memory hints `descriptor` states.
    ///
    /// It is handed no adapter. wgpu-core stores `required_features` off the descriptor and
    /// answers `Device::features` from it, so a hal path that derived a feature set of its own
    /// would give a device capabilities the ordinary path never grants, with nothing in wgpu's API
    /// able to see the difference. With no adapter in scope there is nothing here to derive them
    /// from a second time, and `descriptor` is the one derivation both paths read.
    fn create(
        hal: &wgpu::hal::vulkan::Adapter,
        extensions: &[&'static CStr],
        descriptor: &wgpu::DeviceDescriptor<'_>,
    ) -> Result<wgpu::hal::OpenDevice<wgpu::hal::api::Vulkan>, wgpu::hal::DeviceError> {
        // The names go in the vector. wgpu-hal documents every change to the extension list of
        // `create_info` as overwritten, and builds the list `vkCreateDevice` is given out of the
        // vector.
        let callback: Box<wgpu::hal::vulkan::CreateDeviceCallback<'_>> = Box::new(move |args| {
            for &name in extensions {
                if !args.extensions.contains(&name) {
                    args.extensions.push(name);
                }
            }
        });
        // SAFETY: the callback only appends extension names, every one of which the caller found
        // on this physical device before calling; it removes nothing, disables no feature, and
        // touches no other field of the create info.
        unsafe {
            hal.open_with_callback(
                descriptor.required_features,
                &descriptor.required_limits,
                &descriptor.memory_hints,
                Some(callback),
            )
        }
    }
}

/// The stand-in, on the targets whose wgpu has no Vulkan backend to reach.
#[cfg(not(vulkan_hal))]
mod absent {
    use std::ffi::CStr;

    use crate::gpu::extensions::Opened;

    /// Returns `true` where a device made from `adapter` would enable every one of `extensions`.
    ///
    /// Only for the empty list, on a target with no Vulkan backend.
    pub(crate) fn granted_by(_adapter: &wgpu::Adapter, extensions: &[&'static CStr]) -> bool {
        extensions.is_empty()
    }

    /// Opens a device with every one of `extensions` enabled, and answers the names it carries.
    ///
    /// Always `None` here, which sends the caller down the ordinary path. It is the answer a GL
    /// adapter gets on a target that does link a Vulkan backend.
    pub(crate) fn open(
        _adapter: &wgpu::Adapter,
        _extensions: &[&'static CStr],
        _descriptor: &wgpu::DeviceDescriptor<'_>,
    ) -> Option<Opened> {
        None
    }
}
