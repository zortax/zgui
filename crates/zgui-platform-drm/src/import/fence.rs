//! A frame's completion, as a descriptor the kernel can wait on.
//!
//! A display engine reads a buffer at the vertical blank after the commit that named it, and the
//! frame the graphics device draws into that buffer can still be unfinished then. Something has to
//! wait for it. The kernel does the wait when the plane is given an `IN_FENCE_FD`: a **sync file**,
//! a descriptor that signals when the work behind it is done. With no such descriptor the wait
//! falls to the thread that commits, which is the thread that also reads input.
//!
//! # From a submission to a descriptor
//!
//! A binary semaphore created with `VkExportSemaphoreCreateInfo` naming `SYNC_FD`, signalled by the
//! submission that ends the frame, and exported with `vkGetSemaphoreFdKHR`. The export has the same
//! effect on the semaphore as a wait: the payload leaves it for the descriptor and the semaphore is
//! unsignalled again, ready for the next frame. So one semaphore serves a whole display.
//!
//! The signal has to be submitted before the export, and it need not have run. A sync file
//! describes work that is on its way, and that is what the kernel is handed.
//!
//! # Where this cannot be done
//!
//! `vkGetPhysicalDeviceExternalSemaphoreProperties` answers whether the driver exports a `SYNC_FD`
//! at all. A driver that does not is an ordinary machine, and the caller keeps the wait it already
//! had.

use std::os::fd::{FromRawFd, OwnedFd};

use ash::{khr, vk};

use crate::import::Unsupported;

/// The one external handle type a display engine can be told to wait on.
///
/// A sync file is the kernel's own object, and `IN_FENCE_FD` is a descriptor for one. An opaque
/// handle names a Vulkan object and reaches nothing outside the driver.
const SYNC_FD: vk::ExternalSemaphoreHandleTypeFlags = vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD;

/// The semaphore a released frame signals, and the entry point that turns it into a descriptor.
///
/// One per display. [`Signal::export`] answers the descriptor for the submission that signalled the
/// semaphore, and leaves the semaphore ready for the next frame.
///
/// The device it was created on has to outlive this. This holds no reference to that device, and
/// [`Signal::destroy`] is handed one back, so the owner of a [`Signal`] keeps that order.
pub(crate) struct Signal {
    /// `vkGetSemaphoreFdKHR`, resolved once.
    exporter: khr::external_semaphore_fd::Device,
    /// The semaphore itself.
    semaphore: vk::Semaphore,
    /// Whether a descriptor still comes out of it.
    ///
    /// Cleared by an export the driver refused. From then on nothing signals the semaphore and it
    /// waits only to be destroyed, and the caller does the wait itself. A binary semaphore
    /// signalled twice with no wait between is undefined, so a refusal has to stop the signalling
    /// as well as the exporting.
    exporting: bool,
}

impl Signal {
    /// Returns a semaphore this device can export a sync file from, or `None` where it cannot.
    ///
    /// `Ok(None)` is the ordinary answer for a driver that exports no sync file. It states a fact
    /// about a machine, and the caller answers it by waiting for the device itself.
    ///
    /// # Errors
    ///
    /// Returns [`Unsupported::Driver`] when the driver refuses the semaphore.
    pub(crate) fn create(
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        device: &ash::Device,
    ) -> Result<Option<Self>, Unsupported> {
        if !exportable(instance, physical) {
            return Ok(None);
        }

        let mut export = vk::ExportSemaphoreCreateInfo::default().handle_types(SYNC_FD);
        let info = vk::SemaphoreCreateInfo::default().push_next(&mut export);
        // SAFETY: both structures are Vulkan's own with their `sType` set by `default()`, and the
        // export information lives until after the call. The handle type was reported exportable
        // for a binary semaphore immediately above, so it is legal here.
        let semaphore = unsafe { device.create_semaphore(&info, None) }.map_err(|error| {
            Unsupported::Driver {
                step: "creating the semaphore a finished frame signals",
                reason: format!("the driver answered {error:?}"),
            }
        })?;

        Ok(Some(Self {
            exporter: khr::external_semaphore_fd::Device::new(instance, device),
            semaphore,
            exporting: true,
        }))
    }

    /// Returns the semaphore a submission signals, while a descriptor still comes out of it.
    ///
    /// Answers `None` once an export was refused. That stops the next frame signalling a binary
    /// semaphore nothing is going to wait on.
    pub(crate) fn semaphore(&self) -> Option<vk::Semaphore> {
        self.exporting.then_some(self.semaphore)
    }

    /// Returns the sync file for the submission that signalled this, and leaves it ready for the
    /// next one.
    ///
    /// A signal has to have been **submitted** first. It need not have run: the descriptor answered
    /// describes work that is on its way, and handing that to the kernel is the point.
    ///
    /// Answers `None` where the driver reports -1, which it is allowed to do for a payload that is
    /// already signalled. The kernel then has nothing to wait on, so the frame is committed with no
    /// fence at all.
    ///
    /// # Errors
    ///
    /// Returns [`Unsupported::Driver`] when the driver refuses the export. The caller stops asking
    /// after that, because the semaphore is left holding a signal nothing consumed.
    pub(crate) fn export(&self) -> Result<Option<OwnedFd>, Unsupported> {
        let info = vk::SemaphoreGetFdInfoKHR::default()
            .semaphore(self.semaphore)
            .handle_type(SYNC_FD);
        // SAFETY: the semaphore was created on this device with `SYNC_FD` among its export handle
        // types, and the caller has submitted the signal operation this reads. The structure is
        // Vulkan's own with its `sType` set by `default()`.
        let raw = unsafe { self.exporter.get_semaphore_fd(&info) }.map_err(|error| {
            Unsupported::Driver {
                step: "exporting the sync file the display waits on",
                reason: format!("the driver answered {error:?}"),
            }
        })?;
        if raw < 0 {
            return Ok(None);
        }
        // SAFETY: the driver answered a descriptor it created for this call and holds no copy of
        // it, so this is the only owner. `get_semaphore_fd` reported success, so `raw` is a
        // descriptor rather than a sentinel — the one sentinel it may answer is -1, which is
        // handled above.
        Ok(Some(unsafe { OwnedFd::from_raw_fd(raw) }))
    }

    /// Records that the driver refused an export, so nothing signals this again.
    pub(crate) fn refused(&mut self) {
        self.exporting = false;
    }

    /// Gives the semaphore back.
    ///
    /// `device` has to be the device it was created on, and every submission that named it has to
    /// have finished. Both are the caller's to answer for, so this is a call and not a [`Drop`]:
    /// only the caller knows whether the device is still finishing work.
    ///
    /// # Safety
    ///
    /// No submission may still name this semaphore.
    pub(crate) unsafe fn destroy(&self, device: &ash::Device) {
        // SAFETY: the semaphore was created on this device, and the caller answers for nothing
        // still naming it. A payload left unconsumed by a refused export is no obstacle: a
        // semaphore may be destroyed signalled.
        unsafe { device.destroy_semaphore(self.semaphore, None) };
    }
}

/// Returns `true` when this physical device exports a binary semaphore as a sync file.
///
/// The question goes to the **physical device**, and an enabled extension is a different fact: the
/// extension says the entry point exists, and this says whether the driver answers it for this
/// handle type. Creating a semaphore for a handle type the driver does not report `EXPORTABLE` for
/// is undefined behaviour, so the question comes before the semaphore.
///
/// Both halves of the answer are read. `EXPORTABLE` says a payload can leave the driver at all, and
/// the compatible list says this handle type may be named while the semaphore is created. A driver
/// that reports one without the other describes a semaphore that cannot be built.
fn exportable(instance: &ash::Instance, physical: vk::PhysicalDevice) -> bool {
    let info = vk::PhysicalDeviceExternalSemaphoreInfo::default().handle_type(SYNC_FD);
    let mut answered = vk::ExternalSemaphoreProperties::default();
    // SAFETY: the physical device comes from this instance, and both structures are Vulkan's own
    // with their `sType` set by `default()`. The call writes only through the answer, which lives
    // until after it. It is core Vulkan 1.1, which every driver this path reaches is past.
    unsafe {
        instance.get_physical_device_external_semaphore_properties(physical, &info, &mut answered);
    }
    answered
        .external_semaphore_features
        .contains(vk::ExternalSemaphoreFeatureFlags::EXPORTABLE)
        && answered.compatible_handle_types.contains(SYNC_FD)
}

#[cfg(test)]
mod tests {
    //! The handle type that is asked for, which needs no driver, and what this machine answers,
    //! which needs one.

    use super::{SYNC_FD, exportable};
    use crate::import::{EXTENSIONS, vulkan};
    use ash::vk;
    use zgui_render_wgpu::SharedGraphics;

    #[test]
    fn the_handle_type_asked_for_is_the_kernels_own_rather_than_the_drivers() {
        // An opaque handle names a Vulkan object and reaches nothing outside the driver, so a
        // plane handed one would be handed a number that means nothing to the kernel. The two sit
        // beside each other in the same flag set and neither is refused by the type system.
        assert_eq!(SYNC_FD, vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
        assert_ne!(SYNC_FD, vk::ExternalSemaphoreHandleTypeFlags::OPAQUE_FD);
    }

    #[test]
    fn this_machine_says_whether_it_exports_a_sync_file() {
        // The capability query itself, which decides between the two paths. Neither answer can be
        // asserted, because a driver that exports no sync file is an ordinary machine. So this
        // checks that the question is answered at all, and reports which answer came back.
        let test = "this_machine_says_whether_it_exports_a_sync_file";
        let graphics = SharedGraphics::with_extensions(EXTENSIONS.to_vec());
        let Ok(gpu) = graphics.open_gpu() else {
            eprintln!("{test}: no usable graphics device, so nothing was asserted");
            return;
        };

        let asked = vulkan(&gpu, |adapter, _| {
            Ok(exportable(
                adapter.shared_instance().raw_instance(),
                adapter.raw_physical_device(),
            ))
        });
        match asked {
            Ok(exports) => eprintln!(
                "{test}: {} {} a sync file, so a frame here is {}",
                gpu.describe(),
                if exports { "exports" } else { "exports no" },
                if exports {
                    "waited for by the kernel"
                } else {
                    "waited for by the program"
                }
            ),
            Err(refusal) => eprintln!(
                "{test}: this device exports no image at all, so nothing was asserted: {refusal}"
            ),
        }
    }
}
