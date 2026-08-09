//! Buffers the CPU can write, and the descriptors that carry a buffer between drivers.
//!
//! A dumb buffer is the memory this crate can allocate on its own. A dma-buf descriptor is how
//! memory crosses between this device and something else: a graphics API allocates an image,
//! exports it as a descriptor, and this device imports the descriptor as a handle it can scan out
//! of. The frame loop then needs no copy. Both directions are here, because the device that
//! exports and the device that imports are the same kind of object.

use std::ptr::NonNull;

use rustix::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use rustix::fs::OFlags;
use rustix::mm::{MapFlags, ProtFlags};

use crate::device::Device;
use crate::error::{Error, Result};
use crate::format::Format;
use crate::ioctl;
use crate::sys;

/// A buffer the driver allocated and the CPU can write into.
///
/// This is the slow path, and every driver has it. It puts a first frame on a screen before any
/// graphics interoperation exists, and it stays as the fallback for a device where that
/// interoperation cannot be built.
///
/// A buffer released with [`Device::destroy_dumb_buffer`] gives back its mapping and its handle.
/// One that is dropped keeps both: the mapping until the process ends, and the handle until the
/// device is closed. Dropping therefore leaks, and the leak ends with the process.
#[derive(Debug)]
pub struct DumbBuffer {
    /// The GEM handle the driver knows it by.
    handle: u32,
    /// How wide, in pixels.
    width: u32,
    /// How tall, in pixels.
    height: u32,
    /// How many bytes one row takes, which a driver rounds up past the width times the pixel size.
    stride: u32,
    /// How many bytes the whole buffer takes.
    length: usize,
    /// The mapping, while it is mapped.
    mapping: Option<NonNull<u8>>,
}

impl DumbBuffer {
    /// Returns how wide the buffer is, in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Returns how tall the buffer is, in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns how many bytes one row takes.
    ///
    /// A driver rounds this up for its own reasons, so a caller writing pixels steps rows by this
    /// number instead of by the width.
    pub fn stride(&self) -> u32 {
        self.stride
    }

    /// Returns the GEM handle, for building a framebuffer from it.
    ///
    /// The handle is scoped to the device that allocated the buffer, and to this open descriptor.
    /// [`Device::add_framebuffer_from_handles`] names a plane by it, so a caller that wants a
    /// layout modifier or a multi-plane framebuffer goes through here.
    pub fn handle(&self) -> u32 {
        self.handle
    }

    /// Returns the buffer's bytes, for writing pixels into.
    ///
    /// `device` has to be the one that allocated this buffer. A GEM handle is a number in one
    /// open descriptor's own namespace, so the same number on another device names another
    /// object, and this would map that one. Nothing here can tell the two apart.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the driver refuses to give an offset to map, and
    /// [`Error::Unusable`] when the mapping itself fails.
    pub fn bytes(&mut self, device: &Device) -> Result<&mut [u8]> {
        if self.mapping.is_none() {
            let mut request = sys::drm_mode_map_dumb {
                handle: self.handle,
                pad: 0,
                offset: 0,
            };
            ioctl::issue(device.fd(), ioctl::MODE_MAP_DUMB, &mut request)?;

            // SAFETY: the length is what the driver reported when it created the buffer, and the
            // offset is what it just answered with for this handle. The mapping is shared because
            // it is the driver's own memory, and it is unmapped when this buffer is destroyed.
            let pointer = unsafe {
                rustix::mm::mmap(
                    std::ptr::null_mut(),
                    self.length,
                    ProtFlags::READ | ProtFlags::WRITE,
                    MapFlags::SHARED,
                    device.fd(),
                    request.offset,
                )
            }
            .map_err(|errno| {
                Error::Unusable(format!("the dumb buffer could not be mapped: {errno}"))
            })?;

            self.mapping = NonNull::new(pointer.cast());
        }

        let pointer = self
            .mapping
            .ok_or_else(|| Error::Unusable("the dumb buffer mapped to nothing".to_owned()))?;

        // SAFETY: `pointer` came from a successful `mmap` of `self.length` bytes that has not been
        // unmapped — only `Device::destroy_dumb_buffer` unmaps it, and that consumes the buffer,
        // so no mapping can be torn down while this borrow is alive.
        Ok(unsafe { std::slice::from_raw_parts_mut(pointer.as_ptr(), self.length) })
    }
}

/// A buffer another driver or another API allocated, which this device can now name.
///
/// The memory stays where it was allocated. This is the name the device gives it, and it lives in
/// the namespace of the one open descriptor the import went through.
///
/// A buffer released with [`Device::release_imported`] gives its handle back. One that is dropped
/// keeps it until the device is closed, so dropping is a leak bounded by the life of that
/// descriptor.
#[derive(Debug)]
pub struct ImportedBuffer {
    /// The GEM handle the driver knows it by.
    handle: u32,
}

impl ImportedBuffer {
    /// Returns the GEM handle, for building a framebuffer from it.
    ///
    /// [`Device::add_framebuffer_from_handles`] takes this together with the extent, the stride
    /// and the layout modifier. A descriptor carries none of those three, so the caller reads them
    /// from the API that allocated the image.
    pub fn handle(&self) -> u32 {
        self.handle
    }
}

impl Device {
    /// Allocates a buffer the CPU can write into.
    ///
    /// ```no_run
    /// use zgui_drm::{Device, format::Format};
    ///
    /// let device = Device::open_first()?;
    /// let mut buffer = device.create_dumb_buffer(64, 64, Format::XRGB8888)?;
    ///
    /// assert_eq!((buffer.width(), buffer.height()), (64, 64));
    ///
    /// // Rows step by the stride the driver chose, which it may round past the width.
    /// buffer.bytes(&device)?.fill(0xff);
    ///
    /// let framebuffer = device.add_framebuffer(&buffer, Format::XRGB8888)?;
    /// device.remove_framebuffer(framebuffer)?;
    /// device.destroy_dumb_buffer(buffer)?;
    /// # Ok::<(), zgui_drm::Error>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the driver refuses, and [`Error::Unusable`] when the format
    /// is not one that can be scanned out.
    pub fn create_dumb_buffer(
        &self,
        width: u32,
        height: u32,
        format: Format,
    ) -> Result<DumbBuffer> {
        let bits = format
            .bytes_per_pixel()
            .ok_or_else(|| Error::Unusable(format!("{format:?} cannot be scanned out")))?
            * 8;

        let mut request = sys::drm_mode_create_dumb {
            width,
            height,
            bpp: bits,
            ..Default::default()
        };
        ioctl::issue(self.fd(), ioctl::MODE_CREATE_DUMB, &mut request)?;

        Ok(DumbBuffer {
            handle: request.handle,
            width,
            height,
            stride: request.pitch,
            length: usize::try_from(request.size).map_err(|_| {
                Error::Unusable("the driver reported a buffer larger than this machine".to_owned())
            })?,
            mapping: None,
        })
    }

    /// Releases a dumb buffer.
    ///
    /// Taken by value, because the handle is dead afterwards and a second release of it is an
    /// error the type system can prevent. `device` has to be the one that allocated the buffer,
    /// for the reason [`DumbBuffer::bytes`] gives.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the driver refuses. The mapping is given back either way,
    /// because it belongs to this process rather than to the driver. The handle is not: a refused
    /// release leaves it allocated until the device is closed, and the buffer has been consumed,
    /// so nothing can ask again. That happens when `device` is the wrong one.
    pub fn destroy_dumb_buffer(&self, mut buffer: DumbBuffer) -> Result<()> {
        // The driver is asked first, so that a refusal is reported against a buffer this process
        // still holds whole. The mapping goes afterwards either way.
        let mut request = sys::drm_mode_destroy_dumb {
            handle: buffer.handle,
        };
        let released = ioctl::issue(self.fd(), ioctl::MODE_DESTROY_DUMB, &mut request);

        if let Some(pointer) = buffer.mapping.take() {
            // SAFETY: the pointer and the length are the ones the mapping was made with, and
            // nothing holds a slice over it: `bytes` borrows the buffer mutably and this consumes
            // it.
            let _ = unsafe { rustix::mm::munmap(pointer.as_ptr().cast(), buffer.length) };
        }

        released
    }

    /// Exports `buffer` as a dma-buf descriptor.
    ///
    /// The descriptor names the memory the driver allocated, and the buffer stays this device's.
    /// Whatever imports the descriptor reaches the same pixels, so something other than the CPU
    /// can draw into a buffer this device scans out.
    ///
    /// The descriptor carries `DRM_CLOEXEC` and `DRM_RDWR`. `DRM_CLOEXEC` keeps it out of any
    /// process this one execs, so an exported buffer stays inside the program that exported it.
    /// `DRM_RDWR` makes the memory writable through the descriptor, and the importer of a scanout
    /// buffer writes a frame into it. A descriptor exported read-only stays read-only for its
    /// whole life, so the choice belongs here.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the driver refuses this handle, which it may do for one
    /// buffer and allow for another. Returns [`Error::Unusable`] when it reports success and names
    /// no descriptor.
    pub fn export_buffer(&self, buffer: &DumbBuffer) -> Result<OwnedFd> {
        // `DRM_CLOEXEC` and `DRM_RDWR` are `O_CLOEXEC` and `O_RDWR` under other names: `drm.h`
        // defines each as the other, and bindgen reads that header without the one that declares
        // them, so `sys` carries neither.
        let mut request = sys::drm_prime_handle {
            handle: buffer.handle(),
            flags: (OFlags::CLOEXEC | OFlags::RDWR).bits(),
            // A number no descriptor has, so a driver that reports success and writes nothing
            // here is caught by the check below. Zero is standard input, and owning that number
            // would close standard input at the end of this export's life.
            fd: -1,
        };
        ioctl::issue(self.fd(), ioctl::PRIME_HANDLE_TO_FD, &mut request)?;

        if request.fd < 0 {
            return Err(Error::Unusable(
                "the driver exported the buffer and named no descriptor".to_owned(),
            ));
        }

        // SAFETY: the kernel opened this descriptor for this call and nothing else holds it, so
        // owning it here closes it exactly once. The check above is what says the number came from
        // the kernel.
        Ok(unsafe { OwnedFd::from_raw_fd(request.fd) })
    }

    /// Imports a dma-buf descriptor as a buffer this device can scan out of.
    ///
    /// The descriptor stays the caller's. The kernel takes its own reference on the memory behind
    /// it, so the descriptor may be closed as soon as this returns.
    ///
    /// # One handle per memory object
    ///
    /// The kernel counts no references for a GEM handle, and says so where it documents
    /// `DRM_IOCTL_GEM_CLOSE` in `uapi/drm.h`. Importing memory this open descriptor already has a
    /// handle for answers with that same handle, and that covers a buffer this device allocated,
    /// exported and imported again. `DRM_IOCTL_MODE_GETFB2` is the one call that always answers
    /// with a fresh handle.
    ///
    /// So a caller that imports the same memory twice owns one handle and owes one
    /// [`Device::release_imported`]. A caller that releases once per import closes a buffer the
    /// rest of the program is still scanning out, and the handle it left behind names whatever the
    /// driver allocates next.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the driver refuses the descriptor. Memory it has no path to
    /// is refused that way: an image on a second GPU, or a buffer in a form this driver cannot
    /// address.
    pub fn import_buffer(&self, dmabuf: BorrowedFd<'_>) -> Result<ImportedBuffer> {
        let mut request = sys::drm_prime_handle {
            handle: 0,
            // The header states that the flags apply while a handle is turned into a descriptor,
            // so this direction leaves them at zero.
            flags: 0,
            fd: dmabuf.as_raw_fd(),
        };
        ioctl::issue(self.fd(), ioctl::PRIME_FD_TO_HANDLE, &mut request)?;

        Ok(ImportedBuffer {
            handle: request.handle,
        })
    }

    /// Gives an imported buffer's handle back.
    ///
    /// Taken by value, because the handle is dead afterwards and a second release of it is an
    /// error the type system can prevent. The handle may be one that several imports of the same
    /// memory all answered with, so a second release reaches a buffer another part of the program
    /// still holds.
    ///
    /// `self` has to be the device the buffer was imported through, for the reason
    /// [`DumbBuffer::bytes`] gives.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the driver refuses. A handle it has already closed is what
    /// that looks like, and the buffer has been consumed, so nothing can ask again.
    pub fn release_imported(&self, buffer: ImportedBuffer) -> Result<()> {
        let mut request = sys::drm_gem_close {
            handle: buffer.handle,
            pad: 0,
        };
        ioctl::issue(self.fd(), ioctl::GEM_CLOSE, &mut request)
    }
}
