//! Transfer functions: how a frame encodes light, and what the conversion pass needs to undo it.

/// How light is encoded in R′G′B′.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TransferFunction {
    /// ITU-R BT.709, also BT.601 and 10- and 12-bit BT.2020. Shown as display-referred SDR.
    #[default]
    Bt709,
    /// IEC 61966-2-1 sRGB.
    Srgb,
    /// A pure power of 2.2.
    Gamma22,
    /// A pure power of 2.8.
    Gamma28,
    /// Linear light, with 1.0 at reference white.
    Linear,
    /// SMPTE ST 2084 perceptual quantizer: absolute luminance up to 10 000 nits.
    Pq,
    /// ARIB STD-B67 hybrid log-gamma: relative scene light.
    Hlg,
}

/// The luminance of diffuse white in HDR content, in nits, per ITU-R BT.2408.
pub(crate) const REFERENCE_WHITE: f32 = 203.0;

/// The peak luminance an HDR frame is assumed to reach when it states none, in nits.
pub(crate) const DEFAULT_PEAK: f32 = 1000.0;

impl TransferFunction {
    /// Whether the encoding carries high dynamic range.
    pub fn is_hdr(self) -> bool {
        matches!(self, Self::Pq | Self::Hlg)
    }

    /// The decoding the shader applies, and its exponent where it is a pure power.
    ///
    /// Standard-dynamic-range content decodes with the sRGB curve. That is the curve the
    /// compositor encodes with, so content on BT.709 primaries shows unchanged whether or not it
    /// passes through linear light.
    pub(crate) fn shader(self) -> (ShaderTransfer, f32) {
        match self {
            Self::Bt709 | Self::Srgb => (ShaderTransfer::Srgb, 1.0),
            Self::Gamma22 => (ShaderTransfer::Power, 2.2),
            Self::Gamma28 => (ShaderTransfer::Power, 2.8),
            Self::Linear => (ShaderTransfer::Linear, 1.0),
            Self::Pq => (ShaderTransfer::Pq, 1.0),
            Self::Hlg => (ShaderTransfer::Hlg, 1.0),
        }
    }
}

/// The decodings `convert.wgsl` implements, by the number it switches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShaderTransfer {
    Srgb = 0,
    Linear = 1,
    Pq = 2,
    Hlg = 3,
    Power = 4,
}

/// The HLG system gamma for a display of `peak` nits, per ITU-R BT.2100.
pub(crate) fn hlg_system_gamma(peak: f32) -> f32 {
    1.2 + 0.42 * (peak / 1000.0).log10()
}

/// SMPTE ST 2084 inverse EOTF: luminance in nits to a PQ signal.
pub(crate) fn pq_encode(nits: f32) -> f32 {
    const M1: f64 = 0.1593017578125;
    const M2: f64 = 78.84375;
    const C1: f64 = 0.8359375;
    const C2: f64 = 18.8515625;
    const C3: f64 = 18.6875;
    let y = (f64::from(nits) / 10_000.0).clamp(0.0, 1.0).powf(M1);
    ((C1 + C2 * y) / (1.0 + C3 * y)).powf(M2) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pq_encodes_its_reference_points() {
        assert!(pq_encode(0.0) < 1e-6);
        assert!((pq_encode(10_000.0) - 1.0).abs() < 1e-6);
        // 100 nits is code 520 of 1023, 1000 nits is code 769 (ITU-R BT.2100 tables).
        assert!((pq_encode(100.0) * 1023.0 - 520.0).abs() < 1.0);
        assert!((pq_encode(1000.0) * 1023.0 - 769.0).abs() < 1.0);
    }

    #[test]
    fn the_hlg_system_gamma_is_1_2_at_a_thousand_nits() {
        assert!((hlg_system_gamma(1000.0) - 1.2).abs() < 1e-6);
        assert!(hlg_system_gamma(2000.0) > 1.2);
    }
}
