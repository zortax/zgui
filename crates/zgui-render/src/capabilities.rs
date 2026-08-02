//! What the device underneath a renderer can actually do.

/// The optional device features anything above the renderer has to know about.
///
/// Each one is optional on some real device that has to be supported, and each one changes what the
/// display list should contain rather than only how it is drawn — which is why it is published here
/// instead of being handled quietly inside a backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderCapabilities {
    /// Whether text can be antialiased per colour channel.
    ///
    /// It needs dual-source blending, which is optional on some drivers and absent from the
    /// software rasterisers used for testing. Where it is absent, ordinary coverage is emitted
    /// throughout and one line is logged at startup — a pipeline gated on a feature with no
    /// fallback is a renderer that draws no text at all on those devices.
    pub subpixel_text: bool,
    /// Whether the device can run the compute work a path rasteriser needs.
    ///
    /// Where it cannot, a simpler rasteriser is bound instead. That fallback is only real if the
    /// simpler one exists, so it is not an escape hatch to be written later.
    pub vector_compute: bool,
    /// Whether a texture can be viewed under a second, differently encoded format.
    ///
    /// It decides how a surface that offers only an encoded format is handled: with this, the
    /// encode is bypassed by viewing the surface unencoded; without it, it has to be cancelled in
    /// the final copy instead.
    pub mutable_texture_formats: bool,
    /// The largest texture extent the device will create.
    pub max_texture_size: i32,
}

impl RenderCapabilities {
    /// The least capable device that is still worth supporting: no subpixel text, no compute, no
    /// format views, and the smallest texture limit any target device offers.
    pub const MINIMAL: Self = Self {
        subpixel_text: false,
        vector_compute: false,
        mutable_texture_formats: false,
        max_texture_size: 4096,
    };
}
