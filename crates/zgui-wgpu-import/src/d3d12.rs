//! Windows: shared Direct3D 12 resources as textures, ordered after the producer's fence.
//!
//! Media Foundation and the Direct3D video APIs decode into textures on their own device. A
//! producer shares a decoded `NV12` or `P010` texture as an NT handle, signals a shared fence when
//! the frame is written, and hands both handles over. The resource is opened on wgpu's Direct3D 12
//! device and reaches wgpu through its hal as one multi-planar texture. wgpu's queue waits on the
//! fence on the GPU, so no submission after the import reads the frame before the producer's
//! writes finish, and no thread waits on the processor.

use std::sync::Arc;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D12::{ID3D12Fence, ID3D12Resource};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_FORMAT_P010};
use zgui_render_wgpu::wgpu;
use zgui_wgpu::{ColorSpace, Planes, SampleDepth, VideoFrame};

use crate::ImportError;

/// A decoded frame shared by NT handles.
#[derive(Clone, Copy, Debug)]
pub struct SharedFrame {
    /// The shared `NV12` or `P010` texture.
    pub resource: HANDLE,
    /// The shared fence the producer signals when the frame is written.
    pub fence: HANDLE,
    /// The value the fence reaches once the frame is written.
    pub value: u64,
}

/// What a frame keeps alive until the GPU has read it: the opened resource and fence.
struct Held {
    /// The resource, opened on wgpu's device.
    _resource: ID3D12Resource,
    /// The fence, kept so its last wait has a live object to name.
    _fence: ID3D12Fence,
}

// SAFETY: Direct3D 12 objects are free-threaded; their reference counts are atomic, and nothing
// here calls into them after they are moved.
unsafe impl Send for Held {}

/// Imports shared Direct3D 12 frames on one device.
pub struct D3d12Importer {
    /// Whether the device has `P010` textures as well as `NV12` ones.
    p010: bool,
}

impl D3d12Importer {
    /// An importer for `device`.
    ///
    /// # Errors
    ///
    /// [`ImportError::Backend`] on a device that is not a Direct3D 12 one, and
    /// [`ImportError::Missing`] when wgpu's `NV12` textures are absent.
    pub fn new(device: &wgpu::Device) -> Result<Self, ImportError> {
        // SAFETY: the hal device is only asked whether it exists, then released.
        if unsafe { device.as_hal::<wgpu::hal::api::Dx12>() }.is_none() {
            return Err(ImportError::Backend);
        }
        let features = device.features();
        if !features.contains(wgpu::Features::TEXTURE_FORMAT_NV12) {
            return Err(ImportError::Missing("NV12 textures"));
        }
        Ok(Self {
            p010: features.contains(wgpu::Features::TEXTURE_FORMAT_P010),
        })
    }

    /// Wraps `frame` as a video frame on `device`, ordered after the producer's fence on `queue`.
    ///
    /// `color` is the colour space the bitstream states. `guard` is whatever keeps the producer
    /// from writing the texture again; the frame holds it until the conversion pass has read it.
    /// Each call opens the shared handles anew.
    ///
    /// # Errors
    ///
    /// [`ImportError::Format`] for a texture that is neither `NV12` nor `P010`, and
    /// [`ImportError::Platform`] when a handle cannot be opened or the queue refuses the wait.
    ///
    /// # Safety
    ///
    /// `frame.resource` must be an NT handle to a shared 2D texture with one mip level and one
    /// array layer, and `frame.fence` an NT handle to a shared fence; both must be open for the
    /// length of the call.
    pub unsafe fn import(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &SharedFrame,
        color: ColorSpace,
        guard: impl Send + 'static,
    ) -> Result<VideoFrame, ImportError> {
        // SAFETY: the hal device is used for the two opens below and released before return.
        let hal = unsafe { device.as_hal::<wgpu::hal::api::Dx12>() }.ok_or(ImportError::Backend)?;
        let raw = hal.raw_device();
        let mut resource: Option<ID3D12Resource> = None;
        let mut fence: Option<ID3D12Fence> = None;
        // SAFETY: the caller promises both handles name shared objects of these kinds.
        unsafe {
            raw.OpenSharedHandle(frame.resource, &mut resource)
                .map_err(|error| ImportError::Platform(format!("opening the texture: {error}")))?;
            raw.OpenSharedHandle(frame.fence, &mut fence)
                .map_err(|error| ImportError::Platform(format!("opening the fence: {error}")))?;
        }
        drop(hal);
        let (Some(resource), Some(fence)) = (resource, fence) else {
            return Err(ImportError::Platform(
                "a shared handle opened as nothing".to_owned(),
            ));
        };

        // SAFETY: a live resource answers its own description.
        let description = unsafe { resource.GetDesc() };
        let (format, depth) = match description.Format {
            DXGI_FORMAT_NV12 => (wgpu::TextureFormat::NV12, None),
            DXGI_FORMAT_P010 if self.p010 => (wgpu::TextureFormat::P010, Some(SampleDepth::P010)),
            other => return Err(ImportError::Format(format!("DXGI format {}", other.0))),
        };
        let size = wgpu::Extent3d {
            width: description.Width as u32,
            height: description.Height,
            depth_or_array_layers: 1,
        };

        // SAFETY: the queue is read for its command queue and released before return.
        let hal_queue =
            unsafe { queue.as_hal::<wgpu::hal::api::Dx12>() }.ok_or(ImportError::Backend)?;
        // SAFETY: the fence is live, and a wait queued here orders every later submission on
        // wgpu's queue after the producer's writes.
        unsafe { hal_queue.as_raw().Wait(&fence, frame.value) }
            .map_err(|error| ImportError::Platform(format!("waiting on the fence: {error}")))?;
        drop(hal_queue);

        // SAFETY: the resource is a 2D, single-level, single-sampled texture of `format` and this
        // size, created on the device this one shares with.
        let hal_texture = unsafe {
            wgpu::hal::dx12::Device::texture_from_raw(
                resource.clone(),
                format,
                wgpu::TextureDimension::D2,
                size,
                1,
                1,
            )
        };
        // SAFETY: the texture is on this device, matches the descriptor, and holds the producer's
        // samples once the queued wait completes.
        let texture = unsafe {
            device.create_texture_from_hal::<wgpu::hal::api::Dx12>(
                hal_texture,
                &wgpu::TextureDescriptor {
                    label: Some("zgui.video.d3d12"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
            )
        };
        let held = Held {
            _resource: resource,
            _fence: fence,
        };
        let mut frame = VideoFrame::new(Planes::Multiplanar(Arc::new(texture)), color)
            .with_guard((held, guard));
        if let Some(depth) = depth {
            frame = frame.with_depth(depth);
        }
        Ok(frame)
    }
}
