//! Buffers the CPU can write.
//!
//! The descriptors that carry a buffer between drivers belong here too, and arrive with the
//! caller that needs them: the platform backend imports a graphics API's image for scanout, and
//! this crate gains `PRIME_FD_TO_HANDLE` when that backend is written.

use std::ptr::NonNull;

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
    /// error the type system can prevent.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Ioctl`] when the driver refuses.
    pub fn destroy_dumb_buffer(&self, mut buffer: DumbBuffer) -> Result<()> {
        if let Some(pointer) = buffer.mapping.take() {
            // SAFETY: the pointer and the length are the ones the mapping was made with, and
            // nothing holds a slice over it: `bytes` borrows the buffer mutably and this consumes
            // it.
            let _ = unsafe { rustix::mm::munmap(pointer.as_ptr().cast(), buffer.length) };
        }
        let mut request = sys::drm_mode_destroy_dumb {
            handle: buffer.handle,
        };
        ioctl::issue(self.fd(), ioctl::MODE_DESTROY_DUMB, &mut request)
    }
}
