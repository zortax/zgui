//! Hardware-decoded video frames on zgui's device, without a copy.
//!
//! A hardware decoder writes into memory the platform owns: a CoreVideo pixel buffer on macOS, a
//! dma-buf on Linux, a shared Direct3D 12 resource on Windows. Each importer here turns such a
//! frame into a [`VideoFrame`](zgui_wgpu::VideoFrame) whose planes are textures over that same
//! memory. The colour space comes from the frame's own metadata where the platform carries it, and
//! the frame's guard holds the platform buffer until zgui's conversion pass has read it.
//!
//! An importer works on the device it was made for. A producer makes a new one when
//! [`SurfaceEvent::DeviceLost`](zgui_wgpu::SurfaceEvent::DeviceLost) arrives. An importer on a
//! backend it does not serve answers [`ImportError::Backend`], and the producer falls back to
//! reading the frame to memory and uploading it with
//! [`Planes::upload_biplanar`](zgui_wgpu::Planes::upload_biplanar).

// On the unsafe ledger's allowlist: every import calls a platform API and hands the result to wgpu
// through its hal, and both are unsafe. Every unsafe block states what makes it sound.
#![allow(unsafe_code)]

mod error;

#[cfg(target_os = "macos")]
pub mod core_video;
#[cfg(windows)]
pub mod d3d12;
#[cfg(target_os = "linux")]
pub mod dma_buf;

pub use error::ImportError;
