//! A DXGI swap chain this crate creates, acquires from and presents to itself.
//!
//! # Why not wgpu's
//!
//! wgpu makes a `DXGI_SWAP_EFFECT_FLIP_DISCARD` swap chain, whose buffers may lose what they held
//! after each present, and presents with `Present`, which names no dirty rectangles. The desktop
//! window manager then composes the whole window again for every frame.
//!
//! # The buffers keep their contents
//!
//! The swap chain here is `DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL`, whose back buffers keep what was
//! drawn into them. The flip-model documentation makes the application responsible for each back
//! buffer being whole when it is presented with dirty rectangles, so a frame copies what that buffer
//! owes ([`Debt`]) and `Present1` names what changed since the previous present.
//!
//! # wgpu's tracker
//!
//! Each buffer is wrapped once as a `wgpu::Texture`. wgpu tracks it as a colour target after its
//! first frame. A buffer has to be in `D3D12_RESOURCE_STATE_PRESENT` when it is presented, so two
//! command lists per buffer, recorded once, move it to the present state after the frame and back
//! to the render target state before the next frame that draws into it. wgpu makes the first
//! transition of a new buffer itself, from the common state it was created in.
//!
//! # Order on the queue
//!
//! The swap chain is made on the queue wgpu submits frames on, and a present is ordered after
//! everything submitted to that queue before it. The renderer draws on one thread, and every call
//! here is made from it.
//!
//! # Where it is used
//!
//! On a window that has an `HWND`, with an opaque alpha mode and a format with no sRGB encoding. A
//! translucent window needs a composition swap chain, and wgpu keeps those.

use std::sync::Arc;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, RECT, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Graphics::Direct3D12::{
    D3D12_COMMAND_LIST_TYPE_DIRECT, D3D12_FENCE_FLAG_NONE, D3D12_RESOURCE_BARRIER,
    D3D12_RESOURCE_BARRIER_0, D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
    D3D12_RESOURCE_BARRIER_FLAG_NONE, D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
    D3D12_RESOURCE_STATE_PRESENT, D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATES,
    D3D12_RESOURCE_TRANSITION_BARRIER, ID3D12CommandAllocator, ID3D12CommandList,
    ID3D12CommandQueue, ID3D12Device, ID3D12Fence, ID3D12GraphicsCommandList, ID3D12Resource,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM,
    DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, DXGI_CREATE_FACTORY_FLAGS, DXGI_ERROR_DEVICE_REMOVED,
    DXGI_ERROR_DEVICE_RESET, DXGI_FEATURE_PRESENT_ALLOW_TEARING, DXGI_MWA_NO_ALT_ENTER,
    DXGI_MWA_NO_WINDOW_CHANGES, DXGI_PRESENT, DXGI_PRESENT_ALLOW_TEARING, DXGI_PRESENT_PARAMETERS,
    DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_CHAIN_FLAG,
    DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING, DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT,
    DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT, IDXGIFactory2,
    IDXGIFactory5, IDXGISwapChain3,
};
use windows::Win32::System::Threading::{CreateEventW, INFINITE, WaitForSingleObjectEx};
use windows::core::Interface;
use zgui_geom::{Device, Rect, Size};

use crate::gpu::device::Gpu;
use crate::target::acquire::Acquisition;
use crate::target::debt::Debt;
use crate::target::native::{ChainConfig, NativeImage, NativeWindow, clipped_damage};

/// How long an acquisition waits for a buffer, in milliseconds, as wgpu's own does.
const ACQUIRE_TIMEOUT_MS: u32 = 1_000;

/// One buffer of the swap chain, with what moves it between the frame and the compositor.
struct Buffer {
    /// The buffer, as wgpu sees it.
    texture: wgpu::Texture,
    /// The list that takes the buffer back from the present state.
    again: ID3D12GraphicsCommandList,
    /// The list that gives the buffer to the present state.
    release: ID3D12GraphicsCommandList,
    /// The allocator both lists were recorded from.
    _allocator: ID3D12CommandAllocator,
    /// Whether a frame was drawn into the buffer since the swap chain was made or resized.
    used: bool,
}

/// A DXGI swap chain owned by this crate.
pub(crate) struct Chain {
    /// The device, kept alive for as long as anything here exists on it.
    gpu: Arc<Gpu>,
    /// The D3D12 device.
    device: ID3D12Device,
    /// The queue frames are submitted on, which the swap chain presents on.
    queue: ID3D12CommandQueue,
    /// The factory the swap chain is made from.
    factory: IDXGIFactory2,
    /// The window.
    hwnd: HWND,
    /// Whether a present may tear.
    tearing: bool,
    /// The swap chain, `None` before the first configuration.
    swapchain: Option<IDXGISwapChain3>,
    /// What the swap chain signals when it can take another frame.
    waitable: Option<HANDLE>,
    /// The flags the swap chain was made with, which every resize has to repeat.
    flags: DXGI_SWAP_CHAIN_FLAG,
    /// The buffers of the swap chain.
    buffers: Vec<Buffer>,
    /// What waits for the queue to finish.
    fence: ID3D12Fence,
    /// The last value the fence was told to reach.
    fenced: u64,
    /// The event a wait for the fence blocks on.
    event: HANDLE,
    /// The presentation mode.
    mode: wgpu::PresentMode,
    /// The extent of the buffers.
    extent: Size<i32, Device>,
    /// What each buffer owes the composed target.
    debt: Debt,
    /// The frame extent of the last present, or `None` when the next present has to name the
    /// whole frame.
    presented: Option<Size<i32, Device>>,
}

// SAFETY: the window handle and the two event handles are kernel handles, which any thread of the
// process may use. The DXGI and D3D12 objects are free-threaded. The renderer moves a surface to a
// worker thread to configure it, and nothing here is used from two threads at once.
unsafe impl Send for Chain {}

impl std::fmt::Debug for Chain {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("dxgi::Chain")
            .field("buffers", &self.buffers.len())
            .field("extent", &self.extent)
            .finish_non_exhaustive()
    }
}

impl Chain {
    /// A chain for `window`, or `None` where the device or the window cannot have one.
    pub(crate) fn new(gpu: &Arc<Gpu>, window: NativeWindow) -> Option<Self> {
        let hwnd = HWND(window.hwnd? as *mut core::ffi::c_void);
        // SAFETY: `as_hal` asks that the device is not destroyed while the guard lives. The guard
        // is read through and dropped here, and the COM references cloned out of it keep what they
        // name alive.
        let (device, queue) = unsafe {
            let hal = gpu.device().as_hal::<wgpu::hal::api::Dx12>()?;
            (hal.raw_device().clone(), hal.raw_queue().clone())
        };
        // SAFETY: no flags, and the interface asked for is one a factory implements.
        let factory: IDXGIFactory2 =
            unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)) }.ok()?;
        let tearing = allows_tearing(&factory);
        // SAFETY: the structures are plain values and the device is live.
        let fence: ID3D12Fence = unsafe { device.CreateFence(0, D3D12_FENCE_FLAG_NONE) }.ok()?;
        // SAFETY: an unnamed auto-reset event with default security.
        let event = unsafe { CreateEventW(None, false, false, None) }.ok()?;
        tracing::info!(
            tearing,
            "a window presents through a swap chain this renderer made itself"
        );
        Some(Self {
            gpu: Arc::clone(gpu),
            device,
            queue,
            factory,
            hwnd,
            tearing,
            swapchain: None,
            waitable: None,
            flags: DXGI_SWAP_CHAIN_FLAG(0),
            buffers: Vec::new(),
            fence,
            fenced: 0,
            event,
            mode: wgpu::PresentMode::Fifo,
            extent: Size::new(0, 0),
            debt: Debt::new(0),
            presented: None,
        })
    }

    /// Makes the swap chain, or resizes it, for `config`.
    pub(crate) fn configure(&mut self, config: &ChainConfig) -> Result<(), String> {
        if config.alpha_mode != wgpu::CompositeAlphaMode::Opaque {
            return Err("a translucent window needs a composition swap chain".to_owned());
        }
        if !config.view_formats.is_empty() {
            return Err("a flip-model swap chain is not viewed in another format here".to_owned());
        }
        let format = dxgi_format(config.format)
            .ok_or_else(|| format!("{:?} has no flip-model format", config.format))?;
        let width = config.size.width.max(1) as u32;
        let height = config.size.height.max(1) as u32;
        self.mode = config.present_mode;

        // Nothing in flight may still name a buffer this gives back.
        self.idle();
        self.buffers.clear();

        let swapchain = match &self.swapchain {
            Some(swapchain) => {
                // SAFETY: the queue is idle and every reference to a buffer was dropped above.
                unsafe {
                    swapchain.ResizeBuffers(0, width, height, DXGI_FORMAT_UNKNOWN, self.flags)
                }
                .map_err(|error| format!("resizing the swap chain: {error}"))?;
                swapchain.clone()
            }
            None => self.create(width, height, format)?,
        };
        self.swapchain = Some(swapchain.clone());

        // SAFETY: the swap chain is live.
        let count = unsafe { swapchain.GetDesc1() }
            .map_err(|error| format!("reading the swap chain: {error}"))?
            .BufferCount;
        for index in 0..count {
            // SAFETY: `index` is below the swap chain's own buffer count.
            let resource: ID3D12Resource = unsafe { swapchain.GetBuffer(index) }
                .map_err(|error| format!("a swap chain buffer: {error}"))?;
            let buffer = self.wrap(resource, config.format, width, height)?;
            self.buffers.push(buffer);
        }
        self.extent = Size::new(width as i32, height as i32);
        self.debt = Debt::new(self.buffers.len());
        self.presented = None;
        Ok(())
    }

    /// Waits until the swap chain can take another frame, and takes the buffer it goes into.
    pub(crate) fn acquire(&mut self) -> (Acquisition, Option<NativeImage>) {
        let Some(swapchain) = &self.swapchain else {
            return (Acquisition::Outdated, None);
        };
        if let Some(waitable) = self.waitable {
            // SAFETY: the handle is the swap chain's own and stays open while it lives.
            let waited = unsafe { WaitForSingleObjectEx(waitable, ACQUIRE_TIMEOUT_MS, true) };
            if waited == WAIT_TIMEOUT {
                return (Acquisition::Timeout, None);
            }
        }
        // SAFETY: the swap chain is live.
        let slot = unsafe { swapchain.GetCurrentBackBufferIndex() } as usize;
        let Some(buffer) = self.buffers.get_mut(slot) else {
            return (Acquisition::Validation, None);
        };
        if buffer.used {
            let list: ID3D12CommandList = match buffer.again.cast() {
                Ok(list) => list,
                Err(_) => return (Acquisition::Validation, None),
            };
            // SAFETY: the list is closed and names a buffer of this swap chain, which the frame
            // before last released to the present state.
            unsafe { self.queue.ExecuteCommandLists(&[Some(list)]) };
        }
        buffer.used = true;
        (
            Acquisition::Success,
            Some(NativeImage {
                slot,
                texture: buffer.texture.clone(),
                readable: true,
            }),
        )
    }

    /// Records rectangles a frame wrote into the composed target.
    pub(crate) fn note_written(&mut self, rects: &[Rect<i32, Device>]) {
        self.debt.note_written(rects);
    }

    /// Takes what the buffer at `slot` owes, inside a frame of `frame` pixels.
    pub(crate) fn take_owed(
        &mut self,
        slot: usize,
        frame: Size<i32, Device>,
    ) -> Vec<Rect<i32, Device>> {
        let frame = Size::new(
            frame.width.min(self.extent.width),
            frame.height.min(self.extent.height),
        );
        self.debt.take(slot, frame, false)
    }

    /// Releases the buffer at `slot` and presents it.
    ///
    /// Answers what the present said, and how many pixels it named as changed.
    pub(crate) fn present(
        &mut self,
        slot: usize,
        frame: Size<i32, Device>,
        damage: &[Rect<i32, Device>],
    ) -> (Acquisition, u64) {
        let (Some(swapchain), Some(buffer)) = (&self.swapchain, self.buffers.get(slot)) else {
            return (Acquisition::Validation, 0);
        };
        let Ok(list) = buffer.release.cast::<ID3D12CommandList>() else {
            return (Acquisition::Validation, 0);
        };
        // SAFETY: the list is closed and names the buffer the frame was just copied into.
        unsafe { self.queue.ExecuteCommandLists(&[Some(list)]) };

        let full = self.presented != Some(frame);
        let frame = Size::new(
            frame.width.min(self.extent.width),
            frame.height.min(self.extent.height),
        );
        let mut rects: Vec<RECT> = if full {
            Vec::new()
        } else {
            clipped_damage(damage, frame)
                .iter()
                .map(|rect| RECT {
                    left: rect.origin.x,
                    top: rect.origin.y,
                    right: rect.origin.x + rect.size.width,
                    bottom: rect.origin.y + rect.size.height,
                })
                .collect()
        };
        let parameters = DXGI_PRESENT_PARAMETERS {
            DirtyRectsCount: rects.len() as u32,
            pDirtyRects: if rects.is_empty() {
                core::ptr::null_mut()
            } else {
                rects.as_mut_ptr()
            },
            pScrollRect: core::ptr::null_mut(),
            pScrollOffset: core::ptr::null_mut(),
        };
        // As wgpu presents: tearing only for `Immediate`, and only where the system allows it.
        let (interval, flags) = match self.mode {
            wgpu::PresentMode::Immediate if self.tearing => (0, DXGI_PRESENT_ALLOW_TEARING),
            wgpu::PresentMode::Immediate | wgpu::PresentMode::Mailbox => (0, DXGI_PRESENT(0)),
            _ => (1, DXGI_PRESENT(0)),
        };
        // SAFETY: the parameters point at `rects`, which outlives the call, and every rectangle
        // lies inside the buffer.
        let result = unsafe { swapchain.Present1(interval, flags, &parameters) };
        let damaged = if rects.is_empty() {
            frame.width.max(0) as u64 * frame.height.max(0) as u64
        } else {
            rects
                .iter()
                .map(|rect| (rect.right - rect.left) as u64 * (rect.bottom - rect.top) as u64)
                .sum()
        };
        if result == DXGI_ERROR_DEVICE_REMOVED || result == DXGI_ERROR_DEVICE_RESET {
            self.presented = None;
            self.gpu.loss().report(
                wgpu::DeviceLostReason::Unknown,
                "the device was removed while presenting",
            );
            return (Acquisition::Validation, 0);
        }
        if result.is_err() {
            self.presented = None;
            tracing::warn!(%result, "a present was refused");
            return (Acquisition::Validation, 0);
        }
        self.presented = Some(frame);
        (Acquisition::Success, damaged)
    }

    /// Makes the swap chain.
    fn create(
        &mut self,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> Result<IDXGISwapChain3, String> {
        let mut flags = DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT;
        // Set at creation whatever the mode, because a resize cannot change it.
        if self.tearing {
            flags |= DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING;
        }
        let description = DXGI_SWAP_CHAIN_DESC1 {
            Width: width,
            Height: height,
            Format: format,
            Stereo: false.into(),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: crate::gpu::surface::FRAME_LATENCY + 1,
            Scaling: DXGI_SCALING_STRETCH,
            SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            AlphaMode: DXGI_ALPHA_MODE_IGNORE,
            Flags: flags.0 as u32,
        };
        // SAFETY: the queue is the device's own, the window handle is live for as long as the
        // surface wgpu made for it, and the description is complete.
        let swapchain = unsafe {
            self.factory
                .CreateSwapChainForHwnd(&self.queue, self.hwnd, &description, None, None)
        }
        .map_err(|error| format!("creating the swap chain: {error}"))?;
        let swapchain: IDXGISwapChain3 = swapchain
            .cast()
            .map_err(|error| format!("the swap chain has no third interface: {error}"))?;
        // SAFETY: the window is the one the swap chain was made for. Leaving Alt+Enter to the
        // application is what wgpu does too.
        unsafe {
            self.factory.MakeWindowAssociation(
                self.hwnd,
                DXGI_MWA_NO_WINDOW_CHANGES | DXGI_MWA_NO_ALT_ENTER,
            )
        }
        .map_err(|error| format!("associating the window: {error}"))?;
        // SAFETY: the swap chain was made with the waitable object flag, and the latency is in the
        // range the call allows.
        unsafe { swapchain.SetMaximumFrameLatency(crate::gpu::surface::FRAME_LATENCY) }
            .map_err(|error| format!("setting the frame latency: {error}"))?;
        // SAFETY: as above.
        self.waitable = Some(unsafe { swapchain.GetFrameLatencyWaitableObject() });
        self.flags = flags;
        Ok(swapchain)
    }

    /// Wraps one buffer for wgpu, and records the lists that move it.
    fn wrap(
        &self,
        resource: ID3D12Resource,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<Buffer, String> {
        // SAFETY: the device is live and the type is a direct list's.
        let allocator: ID3D12CommandAllocator = unsafe {
            self.device
                .CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT)
        }
        .map_err(|error| format!("a command allocator: {error}"))?;
        let again = self.record(
            &allocator,
            &resource,
            D3D12_RESOURCE_STATE_PRESENT,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
        )?;
        let release = self.record(
            &allocator,
            &resource,
            D3D12_RESOURCE_STATE_RENDER_TARGET,
            D3D12_RESOURCE_STATE_PRESENT,
        )?;

        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        // SAFETY: the resource is a buffer of a swap chain made on this device, of this format,
        // extent and one level and sample. The texture holds a COM reference, which keeps the
        // buffer alive until wgpu drops it.
        let hal_texture = unsafe {
            wgpu::hal::dx12::Device::texture_from_raw(
                resource,
                format,
                wgpu::TextureDimension::D2,
                size,
                1,
                1,
            )
        };
        let descriptor = wgpu::TextureDescriptor {
            label: Some("zgui.native.buffer"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        };
        // SAFETY: the hal texture was made from this device's own buffer immediately above and
        // respects `descriptor`. A buffer of a new or resized swap chain is in the common state,
        // which `UNINITIALIZED` states.
        let texture = unsafe {
            self.gpu
                .device()
                .create_texture_from_hal::<wgpu::hal::api::Dx12>(
                    hal_texture,
                    &descriptor,
                    wgpu::TextureUses::UNINITIALIZED,
                )
        };
        Ok(Buffer {
            texture,
            again,
            release,
            _allocator: allocator,
            used: false,
        })
    }

    /// Records one transition of `resource` from `before` to `after` into a closed list.
    fn record(
        &self,
        allocator: &ID3D12CommandAllocator,
        resource: &ID3D12Resource,
        before: D3D12_RESOURCE_STATES,
        after: D3D12_RESOURCE_STATES,
    ) -> Result<ID3D12GraphicsCommandList, String> {
        // SAFETY: the allocator is the device's own and of the direct type.
        let list: ID3D12GraphicsCommandList = unsafe {
            self.device
                .CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, allocator, None)
        }
        .map_err(|error| format!("a command list: {error}"))?;
        let barrier = D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                Transition: core::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                    // SAFETY: a borrowed copy of the reference, with no reference count taken.
                    // `ManuallyDrop` keeps it from being released, and the list keeps the barrier
                    // only while it is recorded, while `resource` is borrowed.
                    pResource: unsafe { core::mem::transmute_copy(resource) },
                    Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                    StateBefore: before,
                    StateAfter: after,
                }),
            },
        };
        // SAFETY: the list is open, and the barrier names a live resource.
        unsafe {
            list.ResourceBarrier(&[barrier]);
            list.Close()
        }
        .map_err(|error| format!("closing a command list: {error}"))?;
        Ok(list)
    }

    /// Waits until the queue has finished everything submitted to it.
    fn idle(&mut self) {
        if let Err(error) = self.gpu.device().poll(wgpu::PollType::wait_indefinitely()) {
            tracing::warn!(%error, "waiting for the device before a swap chain was changed");
        }
        self.fenced += 1;
        // SAFETY: the fence and the event belong to this chain, and the queue is live.
        let waited = unsafe {
            self.queue
                .Signal(&self.fence, self.fenced)
                .and_then(|()| self.fence.SetEventOnCompletion(self.fenced, self.event))
                .map(|()| WaitForSingleObjectEx(self.event, INFINITE, false))
        };
        if !matches!(waited, Ok(event) if event == WAIT_OBJECT_0) {
            tracing::warn!("waiting for the queue before a swap chain was changed failed");
        }
    }
}

impl Drop for Chain {
    fn drop(&mut self) {
        self.idle();
        self.buffers.clear();
        self.swapchain = None;
        // SAFETY: both handles were opened by this chain and nothing waits on them now.
        unsafe {
            if let Some(waitable) = self.waitable.take() {
                drop(CloseHandle(waitable));
            }
            drop(CloseHandle(self.event));
        }
    }
}

/// Whether presents may tear on this system.
fn allows_tearing(factory: &IDXGIFactory2) -> bool {
    let Ok(factory) = factory.cast::<IDXGIFactory5>() else {
        return false;
    };
    let mut allowed = windows::core::BOOL(0);
    // SAFETY: the data is a `BOOL`, as the feature asks, and its size is stated.
    let asked = unsafe {
        factory.CheckFeatureSupport(
            DXGI_FEATURE_PRESENT_ALLOW_TEARING,
            (&raw mut allowed).cast(),
            size_of::<windows::core::BOOL>() as u32,
        )
    };
    asked.is_ok() && allowed.as_bool()
}

/// The flip-model format of a swap chain format this crate draws into.
fn dxgi_format(format: wgpu::TextureFormat) -> Option<DXGI_FORMAT> {
    Some(match format {
        wgpu::TextureFormat::Bgra8Unorm => DXGI_FORMAT_B8G8R8A8_UNORM,
        wgpu::TextureFormat::Rgba8Unorm => DXGI_FORMAT_R8G8B8A8_UNORM,
        wgpu::TextureFormat::Rgb10a2Unorm => DXGI_FORMAT_R10G10B10A2_UNORM,
        _ => return None,
    })
}
