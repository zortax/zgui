//! How codes sit in a plane's samples, and where chroma samples sit on the luma grid.

use crate::wgpu;

/// Where a code's bits sit in a 16-bit sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Packing {
    /// In the low bits, as dav1d and libavcodec write 10- and 12-bit planes.
    Low,
    /// In the high bits, as P010 and P016 store them.
    High,
}

/// How many bits a code has, and where they sit in its sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SampleDepth {
    /// The code's bits, from 8 to 16.
    pub bits: u8,
    /// Where the bits sit when the sample is wider than the code.
    pub packing: Packing,
}

impl SampleDepth {
    /// 8-bit codes.
    pub const EIGHT: Self = Self::new(8, Packing::Low);
    /// 10-bit codes in the low bits of 16, as software decoders write them.
    pub const TEN: Self = Self::new(10, Packing::Low);
    /// 12-bit codes in the low bits of 16, as software decoders write them.
    pub const TWELVE: Self = Self::new(12, Packing::Low);
    /// 10-bit codes in the high bits of 16, as P010 stores them.
    pub const P010: Self = Self::new(10, Packing::High);

    /// `bits`-bit codes packed as `packing` says. `bits` is clamped to 8–16.
    pub const fn new(bits: u8, packing: Packing) -> Self {
        let bits = if bits < 8 {
            8
        } else if bits > 16 {
            16
        } else {
            bits
        };
        Self { bits, packing }
    }

    /// The depth a plane of `format` holds when nothing else is stated.
    pub(crate) fn of_format(format: wgpu::TextureFormat) -> Self {
        match container_bits(format) {
            Some(16) => Self::new(16, Packing::Low),
            _ => Self::EIGHT,
        }
    }

    /// The depth a multi-planar texture of `format` holds.
    pub(crate) fn of_multiplanar(format: wgpu::TextureFormat) -> Option<Self> {
        (format == wgpu::TextureFormat::P010).then_some(Self::P010)
    }

    /// What a sampled value of a plane of `format` is multiplied by to read `code / (2^bits − 1)`.
    pub(crate) fn scale(self, format: wgpu::TextureFormat) -> f32 {
        let Some(container) = container_bits(format) else {
            return 1.0;
        };
        let bits = self.bits.min(container);
        let shift = match self.packing {
            Packing::Low => 0,
            Packing::High => container - bits,
        };
        let stored_max = f64::from((1u32 << container) - 1);
        let code_max = f64::from((1u32 << bits) - 1);
        (stored_max / (f64::from(1u32 << shift) * code_max)) as f32
    }
}

/// The bits of one unorm sample of `format`, for the formats a plane can have.
fn container_bits(format: wgpu::TextureFormat) -> Option<u8> {
    match format {
        wgpu::TextureFormat::R8Unorm | wgpu::TextureFormat::Rg8Unorm => Some(8),
        wgpu::TextureFormat::R16Unorm | wgpu::TextureFormat::Rg16Unorm => Some(16),
        _ => None,
    }
}

/// Where each chroma sample sits on the luma grid, when chroma is subsampled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ChromaSiting {
    /// Midway between the luma samples it covers, in both directions. JPEG and most screen
    /// capture use it.
    Center,
    /// On the left luma sample horizontally, midway vertically. H.264, HEVC and MPEG-2 use it.
    #[default]
    Left,
    /// On the top-left luma sample. BT.2020 and AV1's colocated position use it.
    TopLeft,
}

impl ChromaSiting {
    /// The shift, in normalised chroma-plane coordinates, from where bilinear sampling at a luma
    /// position reads to where this siting puts the chroma sample.
    ///
    /// Sampling a chroma plane at the luma plane's normalised coordinates treats every chroma
    /// sample as centred over the luma samples it covers. A cosited sample is a quarter of its
    /// own width further left for 2× subsampling; in general `(s − 1) / 2s` of a chroma sample for
    /// `s`× subsampling. The shift moves the read the other way.
    pub(crate) fn offset(self, luma: (u32, u32), chroma: (u32, u32)) -> [f32; 2] {
        let shift = |luma: u32, chroma: u32| -> f32 {
            let ratio = luma as f32 / chroma.max(1) as f32;
            if ratio <= 1.0 {
                return 0.0;
            }
            ((ratio - 1.0) / (2.0 * ratio)) / chroma as f32
        };
        let horizontal = shift(luma.0, chroma.0);
        let vertical = shift(luma.1, chroma.1);
        match self {
            Self::Center => [0.0, 0.0],
            Self::Left => [horizontal, 0.0],
            Self::TopLeft => [horizontal, vertical],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::TextureFormat::{R8Unorm, R16Unorm};

    #[test]
    fn eight_bit_planes_read_unscaled() {
        assert_eq!(SampleDepth::EIGHT.scale(R8Unorm), 1.0);
    }

    #[test]
    fn low_packed_codes_scale_up_to_their_own_range() {
        // Code 1023 of 10 bits, stored as-is in 16, must read as 1.
        let read = 1023.0 / 65535.0 * SampleDepth::TEN.scale(R16Unorm);
        assert!((read - 1.0).abs() < 1e-6);
    }

    #[test]
    fn high_packed_codes_scale_to_their_own_range() {
        // Code 1023 of 10 bits, stored shifted left by 6.
        let read = (1023.0 * 64.0) / 65535.0 * SampleDepth::P010.scale(R16Unorm);
        assert!((read - 1.0).abs() < 1e-6);
    }

    #[test]
    fn half_width_chroma_cosited_left_shifts_a_quarter_sample() {
        let [x, y] = ChromaSiting::Left.offset((8, 2), (4, 1));
        assert!((x - 0.25 / 4.0).abs() < 1e-6);
        assert_eq!(y, 0.0);
        let [x, y] = ChromaSiting::TopLeft.offset((8, 2), (4, 1));
        assert!((x - 0.25 / 4.0).abs() < 1e-6 && (y - 0.25).abs() < 1e-6);
    }

    #[test]
    fn unsubsampled_chroma_never_shifts() {
        assert_eq!(ChromaSiting::TopLeft.offset((8, 8), (8, 8)), [0.0, 0.0]);
    }
}
