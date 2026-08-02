//! The CSS filter functions, and how far each of them reaches.

/// One entry of a `filter` or `backdrop-filter` chain.
///
/// The distinction that matters throughout is [`Filter::kernel_support`]: a filter that samples only
/// the pixel it is writing costs nothing beyond its own rectangle, while one that samples a
/// neighbourhood makes the content read pixels it never writes. Almost every filter is in the first
/// group, which is why the second is worth singling out rather than treating every group as
/// expensive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Filter {
    /// A Gaussian blur of the given standard deviation, in device pixels.
    Blur(f32),
    /// A blurred, offset copy of the content drawn behind it.
    DropShadow {
        /// How far right the copy is offset, in device pixels.
        offset_x: f32,
        /// How far down the copy is offset, in device pixels.
        offset_y: f32,
        /// The blur's standard deviation, in device pixels.
        blur: f32,
        /// The copy's colour, premultiplied and gamma-encoded.
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
}

impl Filter {
    /// How many standard deviations of a Gaussian blur are visible.
    ///
    /// Three is where the tail falls below one part in a thousand, which is under half a level at
    /// eight bits per channel. Dilating by less cuts the blur off with a visible edge; dilating by
    /// more expands damage for pixels that cannot be seen.
    pub const BLUR_EXTENT: f32 = 3.0;

    /// How far outside a rectangle this filter reads, in device pixels, as
    /// `(left, top, right, bottom)`.
    ///
    /// Zero on every per-pixel filter, and on the overwhelming majority of real chains. A non-zero
    /// answer is what makes a group read pixels it does not write, which is the whole reason
    /// [`read_extent`](crate::read_extent) exists.
    pub fn kernel_support(&self) -> (f32, f32, f32, f32) {
        match self {
            Self::Blur(deviation) => {
                let reach = Self::BLUR_EXTENT * deviation.max(0.0);
                (reach, reach, reach, reach)
            }
            Self::DropShadow {
                offset_x,
                offset_y,
                blur,
                ..
            } => {
                let reach = Self::BLUR_EXTENT * blur.max(0.0);
                (
                    (reach - offset_x).max(0.0),
                    (reach - offset_y).max(0.0),
                    (reach + offset_x).max(0.0),
                    (reach + offset_y).max(0.0),
                )
            }
            _ => (0.0, 0.0, 0.0, 0.0),
        }
    }

    /// Whether this filter reads only the pixel it writes.
    pub fn is_per_pixel(&self) -> bool {
        self.kernel_support() == (0.0, 0.0, 0.0, 0.0)
    }
}
