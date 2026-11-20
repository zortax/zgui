//! Swap chains this crate creates and presents through itself.
//!
//! wgpu discards what a swap chain image held each time one is acquired, and it presents with no
//! word about what changed. So a frame copies its whole composed target onto every image, and the
//! compositor recomposites the whole window, even when one caret blinked.
//!
//! A swap chain made here keeps both. Its images keep their contents between presents, so a frame
//! copies only what the image owes. Its present names the rectangles that changed, so the
//! compositor or the display engine redraws only those.
//!
//! # Where it is used
//!
//! On a Vulkan device, where `VK_KHR_incremental_present` carries the rectangles, and on a DX12
//! device with an opaque window, where `IDXGISwapChain1::Present1` carries them. Every other
//! backend keeps wgpu's swap chain. A native chain that cannot be made gives the window back to
//! wgpu's.
//!
//! It is opt-in, with [`ENV`] set to `native`.

use zgui_geom::{Device, Rect, Size};

use crate::target::acquire::Acquisition;

#[cfg(vulkan_hal)]
mod vulkan;

#[cfg(vulkan_hal)]
pub(crate) use vulkan::INCREMENTAL_PRESENT;

#[cfg(windows)]
mod dxgi;

/// The variable that chooses who owns a window's swap chain.
///
/// `native` makes one here where the backend allows it. Any other value, and no value, leaves it
/// to wgpu.
pub const ENV: &str = "ZGUI_PRESENTER";

/// The variable that checks every presented image against the composed target.
///
/// `1` reads each image back after the copy and logs the first pixel that differs. It costs a
/// readback and a wait per frame, so it is for finding a damage bug, not for running.
pub const VERIFY_ENV: &str = "ZGUI_PRESENT_VERIFY";

/// Whether the environment asks for native swap chains.
pub(crate) fn wanted() -> bool {
    std::env::var(ENV).is_ok_and(|value| value == "native")
}

/// Whether the environment asks for every presented image to be checked.
pub(crate) fn verify() -> bool {
    std::env::var(VERIFY_ENV).is_ok_and(|value| value == "1")
}

/// The configuration a native chain is built for, which mirrors the wgpu surface configuration.
#[derive(Clone, Debug)]
#[cfg_attr(
    not(any(vulkan_hal, windows)),
    expect(dead_code, reason = "no backend here has a native chain to read it")
)]
pub(crate) struct ChainConfig {
    /// The extent of the images.
    pub size: Size<i32, Device>,
    /// The format of the images.
    pub format: wgpu::TextureFormat,
    /// The other formats an image is viewed in.
    pub view_formats: Vec<wgpu::TextureFormat>,
    /// The presentation mode.
    pub present_mode: wgpu::PresentMode,
    /// How the compositor blends the images.
    pub alpha_mode: wgpu::CompositeAlphaMode,
}

/// What a native chain needs to know about the window that the wgpu surface does not say.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct NativeWindow {
    /// The Win32 window handle, where the window has one.
    #[cfg_attr(
        not(windows),
        expect(
            dead_code,
            reason = "only a DXGI swap chain is made for a window handle"
        )
    )]
    pub hwnd: Option<isize>,
}

impl NativeWindow {
    /// What `window` says about itself.
    pub(crate) fn of(window: impl Into<wgpu::SurfaceTarget<'static>>) -> Self {
        use wgpu::rwh::{HasWindowHandle, RawWindowHandle};
        let raw = match window.into() {
            wgpu::SurfaceTarget::DisplayAndWindow(handle) => {
                handle.window_handle().ok().map(|handle| handle.as_raw())
            }
            wgpu::SurfaceTarget::Window(handle) => {
                handle.window_handle().ok().map(|handle| handle.as_raw())
            }
            _ => None,
        };
        let hwnd = match raw {
            Some(RawWindowHandle::Win32(handle)) => Some(handle.hwnd.get()),
            _ => None,
        };
        Self { hwnd }
    }
}

/// An image acquired from a native chain.
#[derive(Debug)]
pub(crate) struct NativeImage {
    /// Which image of the chain it is.
    pub slot: usize,
    /// The image, as wgpu sees it.
    pub texture: wgpu::Texture,
    /// Whether the image can be copied out of, for [`VERIFY_ENV`].
    pub readable: bool,
}

/// A swap chain made here, on one of the backends that can make one.
#[derive(Debug)]
pub(crate) enum NativeChain {
    /// A Vulkan swap chain.
    #[cfg(vulkan_hal)]
    Vulkan(Box<vulkan::Chain>),
    /// A DXGI swap chain.
    #[cfg(windows)]
    Dxgi(Box<dxgi::Chain>),
    /// Nothing, on a target with no backend that has a native chain.
    #[cfg(not(any(vulkan_hal, windows)))]
    #[expect(dead_code, reason = "a value of this variant cannot exist")]
    Never(core::convert::Infallible),
}

#[cfg_attr(
    not(any(vulkan_hal, windows)),
    expect(unused_variables, reason = "no backend here has a native chain")
)]
impl NativeChain {
    /// A chain for `surface` on `gpu`, or `None` where this backend or this surface cannot have
    /// one.
    ///
    /// Makes no swap chain yet. [`NativeChain::configure`] makes the first.
    pub(crate) fn new(
        gpu: &std::sync::Arc<crate::Gpu>,
        surface: &wgpu::Surface<'static>,
        window: NativeWindow,
    ) -> Option<Self> {
        #[cfg(not(windows))]
        let _ = window;
        let backend = gpu.adapter().get_info().backend;
        match backend {
            #[cfg(vulkan_hal)]
            wgpu::Backend::Vulkan => {
                vulkan::Chain::new(gpu, surface).map(|chain| Self::Vulkan(Box::new(chain)))
            }
            #[cfg(windows)]
            wgpu::Backend::Dx12 => {
                dxgi::Chain::new(gpu, window).map(|chain| Self::Dxgi(Box::new(chain)))
            }
            _ => None,
        }
    }

    /// Makes the swap chain again for `config`, replacing the one it held.
    ///
    /// Every image owes everything afterwards, and the next present reports the whole frame as
    /// changed.
    ///
    /// # Errors
    ///
    /// Returns the reason the chain could not be made. The caller then gives the window to wgpu.
    pub(crate) fn configure(&mut self, config: &ChainConfig) -> Result<(), String> {
        match self {
            #[cfg(vulkan_hal)]
            Self::Vulkan(chain) => chain.configure(config),
            #[cfg(windows)]
            Self::Dxgi(chain) => chain.configure(config),
            #[cfg(not(any(vulkan_hal, windows)))]
            Self::Never(never) => match *never {},
        }
    }

    /// Acquires the image the next frame is copied into.
    pub(crate) fn acquire(&mut self) -> (Acquisition, Option<NativeImage>) {
        match self {
            #[cfg(vulkan_hal)]
            Self::Vulkan(chain) => chain.acquire(),
            #[cfg(windows)]
            Self::Dxgi(chain) => chain.acquire(),
            #[cfg(not(any(vulkan_hal, windows)))]
            Self::Never(never) => match *never {},
        }
    }

    /// Records rectangles a frame wrote into the composed target.
    pub(crate) fn note_written(&mut self, rects: &[Rect<i32, Device>]) {
        match self {
            #[cfg(vulkan_hal)]
            Self::Vulkan(chain) => chain.note_written(rects),
            #[cfg(windows)]
            Self::Dxgi(chain) => chain.note_written(rects),
            #[cfg(not(any(vulkan_hal, windows)))]
            Self::Never(never) => match *never {},
        }
    }

    /// Takes what the image at `slot` owes, inside a frame of `frame` pixels.
    pub(crate) fn take_owed(
        &mut self,
        slot: usize,
        frame: Size<i32, Device>,
    ) -> Vec<Rect<i32, Device>> {
        match self {
            #[cfg(vulkan_hal)]
            Self::Vulkan(chain) => chain.take_owed(slot, frame),
            #[cfg(windows)]
            Self::Dxgi(chain) => chain.take_owed(slot, frame),
            #[cfg(not(any(vulkan_hal, windows)))]
            Self::Never(never) => match *never {},
        }
    }

    /// Presents the image at `slot`, which changed `damage` since the frame presented before it.
    ///
    /// `frame` is the extent of the frame inside the image. Answers what the present said, in the
    /// terms an acquisition answers in, and how many pixels it named as changed.
    pub(crate) fn present(
        &mut self,
        slot: usize,
        frame: Size<i32, Device>,
        damage: &[Rect<i32, Device>],
    ) -> (Acquisition, u64) {
        match self {
            #[cfg(vulkan_hal)]
            Self::Vulkan(chain) => chain.present(slot, frame, damage),
            #[cfg(windows)]
            Self::Dxgi(chain) => chain.present(slot, frame, damage),
            #[cfg(not(any(vulkan_hal, windows)))]
            Self::Never(never) => match *never {},
        }
    }
}

/// Clips `rects` to a frame of `frame` pixels, and answers the whole frame for an empty list.
#[cfg_attr(
    not(any(vulkan_hal, windows)),
    expect(dead_code, reason = "no backend here has a native chain to present")
)]
///
/// A present that names no rectangle says nothing changed, which is never what a presented frame
/// means. So an empty list becomes the whole frame.
pub(crate) fn clipped_damage(
    rects: &[Rect<i32, Device>],
    frame: Size<i32, Device>,
) -> Vec<Rect<i32, Device>> {
    let whole = Rect::new(zgui_geom::Point::new(0, 0), frame);
    let clipped: Vec<_> = rects
        .iter()
        .filter_map(|rect| rect.intersection(whole))
        .filter(|rect| !rect.is_empty())
        .collect();
    if clipped.is_empty() {
        vec![whole]
    } else {
        clipped
    }
}
