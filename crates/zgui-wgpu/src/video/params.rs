//! The uniform block and pipeline variant `convert.wgsl` needs for one frame.

use crate::wgpu;

use super::VideoFrame;
use super::color::transfer;
use super::color::{Gamut, affine};

/// Which pipeline converts a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Variant {
    /// Whether the frame passes through linear light: other primaries, or HDR.
    pub mapped: bool,
    /// The format of the converted picture.
    pub output: wgpu::TextureFormat,
}

impl Variant {
    /// The variant for `frame`.
    ///
    /// Deep and HDR frames convert into half floats, so their extra precision survives into the
    /// compositor; everything else converts into 8-bit unorm.
    pub(crate) fn of(frame: &VideoFrame) -> Self {
        let deep = frame.depth().bits > 8 || frame.color.is_hdr();
        Self {
            mapped: !frame.color.is_native(),
            output: if deep {
                wgpu::TextureFormat::Rgba16Float
            } else {
                wgpu::TextureFormat::Rgba8Unorm
            },
        }
    }
}

/// The number of `vec4<f32>` rows in the uniform block.
const ROWS: usize = 11;

/// The uniform block `convert.wgsl` reads for `frame`, as bytes.
pub(crate) fn uniform(frame: &VideoFrame) -> Vec<u8> {
    rows(frame)
        .into_iter()
        .flatten()
        .flat_map(f32::to_ne_bytes)
        .collect()
}

/// The uniform block as rows, in the order `Params` in `convert.wgsl` declares them.
fn rows(frame: &VideoFrame) -> [[f32; 4]; ROWS] {
    let color = frame.color;
    let depth = frame.depth();
    let luma = frame.planes.luma_size();
    let chroma = frame.planes.chroma_size();
    let (width, height) = frame.size();

    let [red, green, blue] = affine(color.matrix, color.range, depth.bits);
    let shape = [
        width as f32 / luma.0 as f32,
        height as f32 / luma.1 as f32,
        f32::from(u8::from(frame.planes.interleaved())),
        depth.scale(frame.planes.luma_format()),
    ];
    let [dx, dy] = frame.siting.offset(luma, chroma);

    let Gamut(gamut) = Gamut::to_bt709(color.primaries);
    let row3 = |r: [f32; 3]| [r[0], r[1], r[2], 0.0];

    let (decoding, gamma) = color.transfer.shader();
    let peak = frame.peak.unwrap_or(transfer::DEFAULT_PEAK).max(1.0);
    let hlg_gamma = transfer::hlg_system_gamma(peak);
    let transfer_row = [
        decoding as u32 as f32,
        gamma,
        hlg_gamma,
        peak / transfer::REFERENCE_WHITE,
    ];

    // BT.2390 EETF, in the PQ domain normalised to the source peak.
    let tone = if color.is_hdr() && peak > transfer::REFERENCE_WHITE {
        let source = transfer::pq_encode(peak);
        let max_lum = transfer::pq_encode(transfer::REFERENCE_WHITE) / source;
        [1.0, source, 1.5 * max_lum - 0.5, max_lum]
    } else {
        [0.0; 4]
    };

    let weights = color.primaries.luminance();
    [
        red,
        green,
        blue,
        shape,
        [dx, dy, 0.0, 0.0],
        row3(gamut[0]),
        row3(gamut[1]),
        row3(gamut[2]),
        transfer_row,
        tone,
        row3(weights),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::color::transfer::ShaderTransfer;
    use crate::video::testing::{device, i420};
    use crate::video::{ColorSpace, SampleDepth, VideoFrame};

    #[test]
    fn the_block_is_whole_rows() {
        let Some((gpu, _held)) = device() else { return };
        let frame = VideoFrame::new(i420(&gpu), ColorSpace::BT709);
        assert_eq!(uniform(&frame).len(), ROWS * 16);
    }

    #[test]
    fn sdr_bt709_skips_linear_light() {
        let Some((gpu, _held)) = device() else { return };
        let frame = VideoFrame::new(i420(&gpu), ColorSpace::BT709);
        assert_eq!(
            Variant::of(&frame),
            Variant {
                mapped: false,
                output: wgpu::TextureFormat::Rgba8Unorm
            }
        );
    }

    #[test]
    fn wide_gamut_and_hdr_pass_through_linear_light() {
        let Some((gpu, _held)) = device() else { return };
        assert!(Variant::of(&VideoFrame::new(i420(&gpu), ColorSpace::BT2020)).mapped);
        let hdr = VideoFrame::new(i420(&gpu), ColorSpace::BT2100_PQ);
        assert_eq!(
            Variant::of(&hdr),
            Variant {
                mapped: true,
                output: wgpu::TextureFormat::Rgba16Float
            }
        );
    }

    #[test]
    fn a_deep_frame_converts_into_half_floats() {
        let Some((gpu, _held)) = device() else { return };
        let frame = VideoFrame::new(i420(&gpu), ColorSpace::BT709).with_depth(SampleDepth::TEN);
        assert_eq!(Variant::of(&frame).output, wgpu::TextureFormat::Rgba16Float);
    }

    #[test]
    fn tone_mapping_is_off_for_sdr_and_on_for_bright_hdr() {
        let Some((gpu, _held)) = device() else { return };
        let sdr = rows(&VideoFrame::new(i420(&gpu), ColorSpace::BT2020));
        assert_eq!(sdr[9][0], 0.0);
        let hdr =
            rows(&VideoFrame::new(i420(&gpu), ColorSpace::BT2100_PQ).with_peak_luminance(4000.0));
        assert_eq!(hdr[9][0], 1.0);
        // The knee sits below the target, and the target below the source.
        assert!(hdr[9][2] < hdr[9][3] && hdr[9][3] < 1.0);
    }

    #[test]
    fn the_shader_numbers_its_decodings_as_rust_does() {
        let source = include_str!("convert.wgsl");
        for decoding in [
            ShaderTransfer::Srgb,
            ShaderTransfer::Linear,
            ShaderTransfer::Pq,
            ShaderTransfer::Hlg,
            ShaderTransfer::Power,
        ] {
            let name = format!("{decoding:?}").to_uppercase();
            let line = format!("const TRANSFER_{name}: u32 = {}u;", decoding as u32);
            assert!(source.contains(&line), "missing `{line}`");
        }
    }
}
