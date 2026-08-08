//! Pixel formats and layout modifiers.
//!
//! `drm_fourcc.h` states these as function-like macros, which bindgen skips. They are written out
//! here and checked against the values the header's own macros expand to.

/// A pixel format, as the four-character code the kernel names it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Format(pub u32);

/// Returns the code these four characters spell.
///
/// This is `fourcc_code` from `drm_fourcc.h`.
const fn fourcc(a: u8, b: u8, c: u8, d: u8) -> Format {
    Format((a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24))
}

impl Format {
    /// 32-bit ARGB, eight bits each.
    pub const ARGB8888: Self = fourcc(b'A', b'R', b'2', b'4');
    /// 32-bit RGB with the top eight bits unused.
    pub const XRGB8888: Self = fourcc(b'X', b'R', b'2', b'4');
    /// 32-bit ABGR, eight bits each.
    pub const ABGR8888: Self = fourcc(b'A', b'B', b'2', b'4');
    /// 32-bit BGR with the top eight bits unused.
    pub const XBGR8888: Self = fourcc(b'X', b'B', b'2', b'4');

    /// Returns how many bytes one pixel of this format takes.
    ///
    /// Only the formats above are answered, because only they are ones this crate hands to a
    /// scanout. Anything else answers `None`, because a guess here would size a buffer wrong.
    pub fn bytes_per_pixel(self) -> Option<u32> {
        match self {
            Self::ARGB8888 | Self::XRGB8888 | Self::ABGR8888 | Self::XBGR8888 => Some(4),
            _ => None,
        }
    }
}

/// A layout modifier: how the pixels of a format are actually arranged in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Modifier(pub u64);

impl Modifier {
    /// Row-major, no tiling, no compression.
    pub const LINEAR: Self = Self(0);
    /// The layout is unknown, and the driver should pick.
    ///
    /// This is `DRM_FORMAT_MOD_INVALID`, and a framebuffer created with it is created without
    /// naming a modifier at all rather than naming this one.
    pub const INVALID: Self = Self(0x00ff_ffff_ffff_ffff);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fourcc_is_its_four_characters_little_endian() {
        // `XR24` is the format almost every scanout in this crate uses, and 0x34325258 is what
        // `fourcc_code('X', 'R', '2', '4')` expands to in the kernel's own header.
        assert_eq!(Format::XRGB8888.0, 0x3432_5258);
        assert_eq!(Format::ARGB8888.0, 0x3432_5241);
        assert_eq!(Format::ABGR8888.0, 0x3432_4241);
        assert_eq!(Format::XBGR8888.0, 0x3432_4258);
    }

    #[test]
    fn the_two_modifiers_that_are_not_vendor_specific_have_their_stated_values() {
        assert_eq!(Modifier::LINEAR.0, 0);
        assert_eq!(Modifier::INVALID.0, (1 << 56) - 1);
    }

    #[test]
    fn only_the_formats_a_scanout_takes_report_a_pixel_size() {
        assert_eq!(Format::XRGB8888.bytes_per_pixel(), Some(4));
        assert_eq!(Format(0).bytes_per_pixel(), None);
    }
}
