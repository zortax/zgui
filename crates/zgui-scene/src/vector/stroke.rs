//! What strokes a path, and in what shape.

use crate::paint::PaintRef;

/// One stroke: what paints it, and every number that decides the outline it stands for.
///
/// The style is `kurbo`'s whole stroke type rather than a width, because a stroke is not a width.
/// A dashed, round-capped, mitre-joined line and a solid butt-capped one of the same width are
/// different pictures, and a display list that carried only the width would hand a rasteriser no
/// way to tell them apart — every stroke would come out solid with nothing anywhere reporting it.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorStroke {
    /// What the stroke is painted with.
    pub paint: PaintRef,
    /// The width, caps, join, miter limit and dashes.
    pub style: kurbo::Stroke,
}

impl VectorStroke {
    /// A solid stroke of the given width, with this framework's defaults for everything else.
    pub fn solid(paint: PaintRef, width: f32) -> Self {
        Self {
            paint,
            style: kurbo::Stroke::new(f64::from(width)),
        }
    }

    /// How wide the stroke is.
    pub fn width(&self) -> f32 {
        self.style.width as f32
    }

    /// How far outside the path's own outline the stroke can put ink.
    ///
    /// Half the width for a butt or round cap and for every join up to the miter limit; a miter
    /// join reaches further, up to the limit times the half width, and a square cap reaches the
    /// half width diagonally. Taking the largest of those is what keeps a mitred corner inside the
    /// rectangle the damage was computed from — under-reporting it would leave the tip of a corner
    /// on the screen after the shape moved away.
    pub fn reach(&self) -> f32 {
        let half = self.style.width.max(0.0) / 2.0;
        let miter = match self.style.join {
            kurbo::Join::Miter => self.style.miter_limit.max(1.0),
            kurbo::Join::Round | kurbo::Join::Bevel => 1.0,
        };
        let squared = |cap| cap == kurbo::Cap::Square;
        let cap = if squared(self.style.start_cap) || squared(self.style.end_cap) {
            std::f64::consts::SQRT_2
        } else {
            1.0
        };
        (half * miter.max(cap)) as f32
    }
}

#[cfg(test)]
mod tests {
    use crate::paint::PaintRef;

    use super::VectorStroke;

    #[test]
    fn a_plain_stroke_reaches_half_its_width() {
        let stroke = VectorStroke::solid(PaintRef::NONE, 8.0);
        assert_eq!(stroke.width(), 8.0);
        assert_eq!(stroke.reach(), 4.0);
    }

    #[test]
    fn a_mitre_join_reaches_past_half_the_width_and_a_round_one_does_not() {
        let mitred = VectorStroke {
            paint: PaintRef::NONE,
            style: kurbo::Stroke::new(8.0)
                .with_join(kurbo::Join::Miter)
                .with_miter_limit(4.0),
        };
        assert_eq!(
            mitred.reach(),
            16.0,
            "a mitre can reach the limit times out"
        );

        let rounded = VectorStroke {
            paint: PaintRef::NONE,
            style: kurbo::Stroke::new(8.0)
                .with_join(kurbo::Join::Round)
                .with_miter_limit(4.0),
        };
        assert_eq!(
            rounded.reach(),
            4.0,
            "a limit that no join uses must not widen the ink"
        );
    }

    #[test]
    fn a_square_cap_reaches_its_corner_rather_than_its_edge() {
        let stroke = VectorStroke {
            paint: PaintRef::NONE,
            style: kurbo::Stroke::new(8.0)
                .with_caps(kurbo::Cap::Square)
                .with_join(kurbo::Join::Round),
        };
        assert!((stroke.reach() - 4.0 * std::f32::consts::SQRT_2).abs() < 1.0e-4);
    }
}
