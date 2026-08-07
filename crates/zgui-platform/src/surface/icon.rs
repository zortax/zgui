//! The picture a desktop shows for a window.

use std::sync::Arc;

/// A window icon, as straight (non-premultiplied) RGBA rows.
///
/// Raw pixels rather than a file: decoding images is not this crate's work, and a backend that took
/// a path would have to grow an image decoder to satisfy a caller that usually already has the
/// pixels. Shared, because the same icon is handed to every window an application opens.
///
/// Several desktops ignore this entirely — a Wayland compositor takes the icon from the
/// application's desktop entry, and macOS from the bundle — so setting one is a request, never a
/// guarantee. See [`Surface::set_icon`](crate::Surface::set_icon).
#[derive(Clone, PartialEq, Eq)]
pub struct WindowIcon {
    /// The pixels, four bytes to a pixel, top row first.
    rgba: Arc<[u8]>,
    /// How wide the picture is, in pixels.
    width: u32,
    /// How tall the picture is, in pixels.
    height: u32,
}

impl WindowIcon {
    /// An icon from straight RGBA rows.
    ///
    /// The length is checked here rather than by the backend, because a mismatch is a caller's
    /// arithmetic error and the place to report it is where the arithmetic was done.
    pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, BadIcon> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(BadIcon::TooLarge)?;
        if rgba.len() != expected {
            return Err(BadIcon::WrongLength {
                expected,
                found: rgba.len(),
            });
        }
        Ok(Self {
            rgba: Arc::from(rgba),
            width,
            height,
        })
    }

    /// The pixels, four bytes to a pixel.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// How wide the picture is, in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// How tall the picture is, in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }
}

impl core::fmt::Debug for WindowIcon {
    /// The shape, never the pixels: an icon printed byte by byte buries whatever was being debugged.
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("WindowIcon")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// Why a picture could not be used as an icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BadIcon {
    /// The pixels do not match the stated size.
    WrongLength {
        /// How many bytes the stated size needs.
        expected: usize,
        /// How many were given.
        found: usize,
    },
    /// The stated size is larger than can be addressed.
    TooLarge,
}

impl core::fmt::Display for BadIcon {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::WrongLength { expected, found } => write!(
                formatter,
                "an icon of this size needs {expected} bytes, and {found} were given"
            ),
            Self::TooLarge => write!(formatter, "the icon is larger than can be addressed"),
        }
    }
}

impl core::error::Error for BadIcon {}

#[cfg(test)]
mod tests {
    use super::{BadIcon, WindowIcon};

    #[test]
    fn an_icon_keeps_the_pixels_it_was_given() {
        let icon = WindowIcon::from_rgba(vec![0xFF; 2 * 2 * 4], 2, 2).expect("the length matches");
        assert_eq!(icon.width(), 2);
        assert_eq!(icon.height(), 2);
        assert_eq!(icon.rgba().len(), 16);
    }

    #[test]
    fn pixels_that_do_not_match_the_size_are_refused_where_the_mistake_was_made() {
        let error = WindowIcon::from_rgba(vec![0xFF; 8], 2, 2).expect_err("the length is wrong");
        assert_eq!(
            error,
            BadIcon::WrongLength {
                expected: 16,
                found: 8
            }
        );
    }
}
