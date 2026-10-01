//! Images already made over a decoder's recycled buffers.
//!
//! A decoder writes into a small pool of buffers and hands each one out again and again. A
//! dma-buf's identity is the inode behind its descriptor, and an inode is not reused while an
//! image this side made still holds its memory. So an image made once over a buffer is the right
//! image every later time the same buffer arrives with the same layout.

use std::collections::VecDeque;
use std::sync::Arc;

use zgui_render_wgpu::wgpu;

use super::DmaBuf;
use crate::ImportError;

/// How many images the cache keeps: a decoder's pool, with room to spare.
const CAPACITY: usize = 32;

/// What makes two imports the same image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Key {
    /// The device and inode of each plane's memory, with the plane's offset and stride.
    planes: Vec<(u64, u64, u32, u32)>,
    /// The four-character code, the modifier and the size.
    layout: (u32, u64, u32, u32),
}

impl Key {
    /// The key of `frame`.
    pub(super) fn of(frame: &DmaBuf<'_>) -> Result<Self, ImportError> {
        let planes = frame
            .planes
            .iter()
            .map(|plane| {
                let stat = rustix::fs::fstat(plane.fd)
                    .map_err(|error| ImportError::Platform(format!("fstat: {error}")))?;
                Ok((stat.st_dev, stat.st_ino, plane.offset, plane.stride))
            })
            .collect::<Result<_, ImportError>>()?;
        Ok(Self {
            planes,
            layout: (frame.fourcc, frame.modifier, frame.width, frame.height),
        })
    }

    /// Whether every plane lives in the same memory.
    pub(super) fn shared(&self) -> bool {
        self.planes
            .windows(2)
            .all(|pair| (pair[0].0, pair[0].1) == (pair[1].0, pair[1].1))
    }
}

/// The images, most recently used last.
#[derive(Default)]
pub(super) struct Cache {
    /// Keys and their images.
    entries: VecDeque<(Key, Arc<wgpu::Texture>)>,
}

impl Cache {
    /// The image made for `key`, if there is one, marked as just used.
    pub(super) fn get(&mut self, key: &Key) -> Option<Arc<wgpu::Texture>> {
        let index = self.entries.iter().position(|(held, _)| held == key)?;
        let entry = self.entries.remove(index)?;
        let texture = Arc::clone(&entry.1);
        self.entries.push_back(entry);
        Some(texture)
    }

    /// Keeps `texture` for `key`, letting the least recently used image go when full.
    pub(super) fn insert(&mut self, key: Key, texture: Arc<wgpu::Texture>) {
        self.entries.retain(|(held, _)| *held != key);
        self.entries.push_back((key, texture));
        while self.entries.len() > CAPACITY {
            self.entries.pop_front();
        }
    }
}
