//! Turning a ramp interpolated in any space into sRGB stops that can be interpolated linearly.
//!
//! The method is adaptive subdivision. Between each pair of author-written stops, the true colour
//! at a few positions inside the interval is compared with the colour a straight line between the
//! two ends would give there; if any of them differ by more than the tolerance the segment is
//! split in half and both halves are examined the same way. Curves that are nearly straight — a
//! short ramp, or one between two similar colours — come back with no extra stops at all, while a
//! ramp across the hue circle gets however many it needs.
//!
//! Several probes rather than one, because three different things put a bend in these curves. A
//! perceptual space bends smoothly, and its worst deviation from a chord is in the middle; but
//! bringing a colour into the sRGB gamut puts a *corner* in the curve wherever a channel reaches
//! its limit, and a corner can sit anywhere in the interval; and a ramp whose alpha approaches zero
//! at one end has a boundary layer there, a hundredth of the interval wide, where undoing the
//! premultiplication divides by almost nothing and the curve turns hard. Evenly spaced probes see
//! neither of the last two reliably, so the probes are also spaced geometrically towards both ends.

#[cfg(test)]
mod tests;

use crate::color::Color;
use crate::gradient::GradientStop;
use crate::interpolate::{Interpolation, interpolate};
use crate::space::ColorSpace;

/// The default per-channel error a densified ramp is held to: half a step of an eight-bit channel.
///
/// Half a step rather than a whole one, so that the error and the rounding to eight bits together
/// cannot move a pixel by more than one value.
pub const DEFAULT_TOLERANCE: f32 = 0.5 / 255.0;

/// How many times a single segment may be halved.
///
/// Twelve halvings is four thousand and ninety-six sub-segments per authored segment, which is the
/// depth the hardest ramps need: a channel driven far out of the sRGB gamut can fall to its limit
/// over a thousandth of the ramp, and a shallower bound would quietly return a ramp outside the
/// tolerance instead of one that meets it. Subdivision is adaptive, so a ramp only pays for the
/// parts of itself that are curved — a full turn of the hue circle at high chroma costs a few
/// hundred stops, not four thousand.
const MAX_DEPTH: u32 = 12;

/// Where inside a candidate segment the approximation is checked, as fractions of it.
///
/// The middle alone is not enough, and nor is any evenly spaced set: the two features that evenly
/// spaced probes miss — a gamut-clipping corner and a near-zero-alpha boundary layer — both like to
/// sit close to an end. The spacing is therefore geometric towards both ends and even across the
/// middle, so that a feature occupying as little as a two-hundred-and-fifty-sixth of the segment is
/// still seen and the segment split around it.
const PROBES: [f32; 17] = [
    1.0 / 256.0,
    1.0 / 128.0,
    1.0 / 64.0,
    1.0 / 32.0,
    1.0 / 16.0,
    1.0 / 8.0,
    1.0 / 4.0,
    3.0 / 8.0,
    1.0 / 2.0,
    5.0 / 8.0,
    3.0 / 4.0,
    7.0 / 8.0,
    15.0 / 16.0,
    31.0 / 32.0,
    63.0 / 64.0,
    127.0 / 128.0,
    255.0 / 256.0,
];

/// Rewrites a gradient's stops so that interpolating them linearly in premultiplied sRGB
/// reproduces the ramp `interpolation` describes.
///
/// The returned stops are all in [`ColorSpace::Srgb`], carry straight (not premultiplied) alpha,
/// and are in the same order as the input, whose offsets must be non-decreasing. Authored stops
/// come through unchanged, so a pair of them at one offset stays a hard transition rather than
/// being smoothed, and a ramp that is already a straight line in sRGB — including one interpolated
/// in sRGB in the first place — gains no stops at all.
///
/// ```
/// use zgui_color::{Color, ColorSpace, GradientStop, Interpolation, densify};
///
/// let stops = [
///     GradientStop::new(0.0, Color::srgb(0.0, 0.0, 1.0, 1.0)),
///     GradientStop::new(1.0, Color::srgb(1.0, 1.0, 0.0, 1.0)),
/// ];
///
/// let oklab = densify(&stops, Interpolation::new(ColorSpace::Oklab));
/// assert!(oklab.len() > 2, "an Oklab ramp is not a straight line in sRGB");
/// assert_eq!(oklab[0].color.space(), ColorSpace::Srgb);
///
/// let srgb = densify(&stops, Interpolation::new(ColorSpace::Srgb));
/// assert_eq!(srgb.len(), 2, "an sRGB ramp is already what it approximates");
/// ```
pub fn densify(stops: &[GradientStop], interpolation: Interpolation) -> Vec<GradientStop> {
    densify_with_tolerance(stops, interpolation, DEFAULT_TOLERANCE)
}

/// [`densify`], with the per-channel error bound stated explicitly.
///
/// `tolerance` is in premultiplied sRGB channel units, where one is the whole range: a tolerance
/// of `1.0 / 255.0` allows one step of an eight-bit channel. Smaller tolerances produce more
/// stops; a tolerance at or below zero produces the most the subdivision limit allows.
pub fn densify_with_tolerance(
    stops: &[GradientStop],
    interpolation: Interpolation,
    tolerance: f32,
) -> Vec<GradientStop> {
    let to_srgb = |stop: &GradientStop| GradientStop::new(stop.offset, srgb(stop.color));
    if stops.len() < 2 {
        return stops.iter().map(to_srgb).collect();
    }

    let mut out = Vec::with_capacity(stops.len());
    out.push(to_srgb(&stops[0]));
    for pair in stops.windows(2) {
        let segment = Segment {
            from: pair[0],
            to: pair[1],
            interpolation,
            tolerance,
        };
        segment.subdivide(&mut out);
    }
    out
}

/// One authored pair of stops, and the rules for approximating the curve between them.
struct Segment {
    /// The stop the segment starts at.
    from: GradientStop,
    /// The stop the segment ends at.
    to: GradientStop,
    /// The space and hue arc the ramp is interpolated in.
    interpolation: Interpolation,
    /// The largest per-channel error the approximation may leave.
    tolerance: f32,
}

impl Segment {
    /// Appends the stops that approximate this segment, not including its first.
    fn subdivide(&self, out: &mut Vec<GradientStop>) {
        // The authored colours are used as they were written rather than as the interpolation
        // reports them at its ends, so a stop the author placed survives untouched and a hard
        // transition between two stops at one offset stays hard.
        let end = GradientStop::new(self.to.offset, srgb(self.to.color));
        let span = self.to.offset - self.from.offset;
        if span <= 0.0 || !span.is_finite() {
            out.push(end);
            return;
        }
        let start = GradientStop::new(self.from.offset, srgb(self.from.color));
        self.refine(0.0, start, 1.0, end, MAX_DEPTH, out);
    }

    /// The stop at `t` of the way along this segment, in sRGB.
    fn sample(&self, t: f32) -> GradientStop {
        let color = interpolate(self.from.color, self.to.color, t, self.interpolation);
        let offset = self.from.offset + (self.to.offset - self.from.offset) * t;
        GradientStop::new(offset, srgb(color))
    }

    /// Appends the stops approximating `start..=end`, not including `start`.
    fn refine(
        &self,
        start_t: f32,
        start: GradientStop,
        end_t: f32,
        end: GradientStop,
        depth: u32,
        out: &mut Vec<GradientStop>,
    ) {
        if depth > 0 && !self.is_close_enough(start_t, start, end_t, end) {
            let middle_t = f32::midpoint(start_t, end_t);
            let middle = self.sample(middle_t);
            self.refine(start_t, start, middle_t, middle, depth - 1, out);
            self.refine(middle_t, middle, end_t, end, depth - 1, out);
            return;
        }
        out.push(end);
    }

    /// Whether the straight line from `start` to `end` stays within the tolerance of the curve.
    fn is_close_enough(
        &self,
        start_t: f32,
        start: GradientStop,
        end_t: f32,
        end: GradientStop,
    ) -> bool {
        let start_color = start.color.to_premultiplied_srgb();
        let end_color = end.color.to_premultiplied_srgb();
        PROBES.into_iter().all(|probe| {
            let exact = self
                .sample(start_t + (end_t - start_t) * probe)
                .color
                .to_premultiplied_srgb();
            (0..4).all(|channel| {
                let chord =
                    start_color[channel] + (end_color[channel] - start_color[channel]) * probe;
                (exact[channel] - chord).abs() <= self.tolerance
            })
        })
    }
}

/// The colour in sRGB, which is the space every densified stop is expressed in.
fn srgb(color: Color) -> Color {
    color.to_space(ColorSpace::Srgb)
}
