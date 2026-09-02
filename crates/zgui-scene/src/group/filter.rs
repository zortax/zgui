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
    /// An application's own shader, reading the content and writing what replaces it.
    Custom {
        /// Which registered effect filters.
        shader: crate::ShaderId,
        /// The parameter block it draws with.
        params: crate::ShaderParamsSlot,
        /// How far outside its rectangle it reads, in device pixels.
        ///
        /// Declared by the effect rather than measured from it: nothing here can look inside a
        /// shader, and a filter that reads further than it said would be fed the previous frame's
        /// output wherever the damage set stopped short.
        reach: f32,
    },
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
            // An effect reads where it said it would. Nothing here can check that, which is why
            // the declaration is the contract: reaching further is a filter fed its own previous
            // output, and the smear that follows is what the extent exists to prevent.
            Self::Custom { reach, .. } => {
                let reach = reach.max(0.0);
                (reach, reach, reach, reach)
            }
            _ => (0.0, 0.0, 0.0, 0.0),
        }
    }

    /// Whether this filter reads only the pixel it writes.
    pub fn is_per_pixel(&self) -> bool {
        self.kernel_support() == (0.0, 0.0, 0.0, 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::Filter;
    use crate::{ShaderId, ShaderParamsSlot};

    /// An effect that reads only where it writes costs its own rectangle and nothing more.
    #[test]
    fn an_effect_that_declared_no_reach_is_per_pixel() {
        let filter = Filter::Custom {
            shader: ShaderId(1),
            params: ShaderParamsSlot(0),
            reach: 0.0,
        };
        assert!(filter.is_per_pixel());
        assert_eq!(filter.kernel_support(), (0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn an_effect_reaches_as_far_as_it_declared_on_every_side() {
        let filter = Filter::Custom {
            shader: ShaderId(1),
            params: ShaderParamsSlot(0),
            reach: 6.0,
        };
        assert!(!filter.is_per_pixel());
        assert_eq!(filter.kernel_support(), (6.0, 6.0, 6.0, 6.0));
    }

    /// A negative reach would shrink the region the content is read from, which is a filter reading
    /// texels the pass never wrote.
    #[test]
    fn a_reach_below_zero_is_treated_as_none() {
        let filter = Filter::Custom {
            shader: ShaderId(1),
            params: ShaderParamsSlot(0),
            reach: -4.0,
        };
        assert_eq!(filter.kernel_support(), (0.0, 0.0, 0.0, 0.0));
    }
}
