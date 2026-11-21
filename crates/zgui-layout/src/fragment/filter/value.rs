//! The CSS filter functions, and how far each one reads.

use zgui_css::shader::ShaderId;

/// One entry of a `filter` or `backdrop-filter` chain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    /// A Gaussian blur of the given standard deviation, in device pixels.
    Blur(f32),
    /// A blurred, offset copy of the content drawn behind it.
    DropShadow {
        /// The horizontal offset of the copy, in device pixels.
        offset_x: f32,
        /// The vertical offset of the copy, in device pixels.
        offset_y: f32,
        /// The standard deviation of the blur, in device pixels.
        blur: f32,
        /// The colour of the copy, premultiplied and gamma-encoded.
        color: [f32; 4],
    },
    /// Scales luminance.
    Brightness(f32),
    /// Scales the distance of each channel from mid grey.
    Contrast(f32),
    /// Interpolates towards the content's luminance.
    Grayscale(f32),
    /// Rotates hue, in radians.
    HueRotate(f32),
    /// Interpolates towards the complement of each channel.
    Invert(f32),
    /// Scales alpha.
    Opacity(f32),
    /// Scales the distance of each channel from the content's luminance.
    Saturate(f32),
    /// Interpolates towards a sepia-toned copy.
    Sepia(f32),
    /// An application effect that reads the content and writes what replaces it.
    Custom {
        /// The declared effect.
        shader: ShaderId,
        /// How far outside its rectangle the effect reads, in device pixels, as it declared.
        reach: f32,
    },
}

impl Filter {
    /// How many standard deviations of a Gaussian blur are visible.
    pub const BLUR_EXTENT: f32 = 3.0;

    /// How far a resampled filter reads beyond its kernel, in device pixels.
    pub const RESAMPLE_REACH: f32 = 2.0;

    /// Returns how far the filter reads outside the pixel it writes, as left, top, right and
    /// bottom distances in device pixels.
    pub fn kernel_support(&self) -> (f32, f32, f32, f32) {
        match self {
            // A blur of zero is the identity, and executing it is skipped.
            Self::Blur(deviation) if deviation.is_nan() || *deviation <= 0.0 => {
                (0.0, 0.0, 0.0, 0.0)
            }
            Self::Blur(deviation) => {
                let reach = Self::BLUR_EXTENT * deviation + Self::RESAMPLE_REACH;
                (reach, reach, reach, reach)
            }
            Self::DropShadow {
                offset_x,
                offset_y,
                blur,
                ..
            } => {
                // The copy is always blurred, so it always resamples.
                let reach = Self::BLUR_EXTENT * blur.max(0.0) + Self::RESAMPLE_REACH;
                (
                    (reach - offset_x).max(0.0),
                    (reach - offset_y).max(0.0),
                    (reach + offset_x).max(0.0),
                    (reach + offset_y).max(0.0),
                )
            }
            Self::Custom { reach, .. } => {
                let reach = reach.max(0.0);
                (reach, reach, reach, reach)
            }
            _ => (0.0, 0.0, 0.0, 0.0),
        }
    }

    /// Reports whether the filter reads only the pixel it writes.
    pub fn is_per_pixel(&self) -> bool {
        self.kernel_support() == (0.0, 0.0, 0.0, 0.0)
    }
}
