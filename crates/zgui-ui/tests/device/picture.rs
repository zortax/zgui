//! A target read back off the device, as red, green, blue and alpha.

use zgui::geom::{Device, Size};

/// The pixels of one target, row-major, four bytes to a pixel in red, green, blue, alpha order.
#[derive(Clone, Debug)]
pub struct Picture {
    bytes: Vec<u8>,
    size: Size<i32, Device>,
}

impl Picture {
    /// Copies the pixels of the device.
    pub fn of_compositor(pixels: &zgui_testkit_compositor::Pixels) -> Self {
        let size = pixels.size();
        Self::copied(pixels.bytes(), pixels.is_bgra(), size.width, size.height)
    }

    /// Copies `bytes`, swapping red and blue when they are stored blue first.
    fn copied(bytes: &[u8], bgra: bool, width: i32, height: i32) -> Self {
        let mut bytes = bytes.to_vec();
        if bgra {
            for pixel in bytes.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        Self {
            bytes,
            size: Size::new(width, height),
        }
    }

    /// Returns the pixel at `(x, y)`.
    ///
    /// # Panics
    ///
    /// Panics if the coordinates lie outside the picture.
    pub fn rgba(&self, x: i32, y: i32) -> [u8; 4] {
        assert!(
            x >= 0 && y >= 0 && x < self.size.width && y < self.size.height,
            "({x}, {y}) is outside the {:?} that was read",
            self.size
        );
        let offset = ((y * self.size.width + x) * 4) as usize;
        [
            self.bytes[offset],
            self.bytes[offset + 1],
            self.bytes[offset + 2],
            self.bytes[offset + 3],
        ]
    }

    pub fn size(&self) -> Size<i32, Device> {
        self.size
    }
}
