//! How many pixels the compositor wants per logical unit, from whichever source it offers.

/// The scale a surface should be drawn at, from the three ways a compositor can say it.
///
/// They are a ladder rather than alternatives, and the order is the protocol's own.
///
/// The **fractional scale** is exact and per surface, reported in hundred-and-twentieths, and it
/// is the only one that can express the 1.25 or 1.5 a desktop actually uses. When it is offered it
/// is the answer and the others are not consulted.
///
/// The **preferred buffer scale** is the compositor's whole-number answer for this surface. It
/// arrived in version 6 of the surface interface.
///
/// The **outputs** the surface is on come next, and the largest scale of them wins: a window
/// straddling a 1× and a 2× monitor drawn at 1× is visibly soft on half of itself, and drawn at
/// 2× is merely oversampled on the other half.
///
/// The **seed** is the last resort. It is the scale of the output the surface was opened on, and
/// it gives the first frame its scale before the compositor has said anything about the surface.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scale {
    /// The fractional scale in hundred-and-twentieths, when the compositor reports one.
    fractional: Option<u32>,
    /// The whole-number scale this surface prefers, when the compositor reports one.
    preferred: Option<i32>,
    /// The largest whole-number scale among the outputs the surface is on.
    outputs: Option<i32>,
    /// The scale of the output the surface is expected on, before it enters one.
    seed: Option<f64>,
}

impl Scale {
    /// What the fractional-scale protocol reports, in hundred-and-twentieths.
    pub const fn fractional(&mut self, scale: u32) {
        self.fractional = Some(scale);
    }

    /// What the surface itself says it prefers.
    pub const fn preferred(&mut self, scale: i32) {
        self.preferred = Some(scale);
    }

    /// The largest scale among the outputs the surface is currently on.
    pub const fn outputs(&mut self, scale: Option<i32>) {
        self.outputs = scale;
    }

    /// The scale of the output the surface is expected on, before it enters one.
    ///
    /// A fraction only for a surface that is mapped through a viewport, because a surface without
    /// one can declare only a whole-number buffer scale.
    pub const fn seed(&mut self, scale: Option<f64>) {
        self.seed = scale;
    }

    /// The scale to draw at, never zero and never negative.
    pub fn factor(&self) -> f64 {
        let answer = match (self.fractional, self.preferred, self.outputs) {
            (Some(fractional), _, _) => f64::from(fractional) / 120.0,
            (None, Some(preferred), _) => f64::from(preferred),
            (None, None, Some(outputs)) => f64::from(outputs),
            (None, None, None) => self.seed.unwrap_or(1.0),
        };
        if answer > 0.0 { answer } else { 1.0 }
    }

    /// Whether the compositor is sizing this surface fractionally.
    ///
    /// A fractional surface is drawn at a whole number of pixels and mapped back to its logical
    /// extent through a viewport, with a buffer scale of one. Reporting the fraction as a buffer
    /// scale is not possible — the request takes an integer — so the two paths are genuinely
    /// different and this is what selects between them.
    pub const fn is_fractional(&self) -> bool {
        self.fractional.is_some()
    }

    /// The whole-number buffer scale to declare, for a surface that is not fractional.
    pub fn buffer_scale(&self) -> i32 {
        self.preferred
            .or(self.outputs)
            .or_else(|| self.seed.map(|seed| seed.round() as i32))
            .unwrap_or(1)
            .max(1)
    }
}

/// Tells the compositor how to read a buffer drawn at `scale`.
///
/// A surface mapped to its logical extent by its viewport declares a buffer scale of one; declaring
/// anything else would apply the scaling twice. A surface without a viewport declares the whole
/// number instead, which is the only scaling available to it.
pub fn declare(surface: &crate::surface::WaylandSurface, scale: f64, viewport: bool) {
    let whole = if viewport {
        1
    } else {
        (scale.round() as i32).max(1)
    };
    surface.wl_surface().set_buffer_scale(whole);
}

#[cfg(test)]
mod tests {
    use super::Scale;

    #[test]
    fn a_compositor_that_says_nothing_gets_one_to_one() {
        let scale = Scale::default();
        assert_eq!(scale.factor(), 1.0);
        assert_eq!(scale.buffer_scale(), 1);
        assert!(!scale.is_fractional());
    }

    #[test]
    fn a_fractional_scale_outranks_everything_else() {
        let mut scale = Scale::default();
        scale.preferred(2);
        scale.outputs(Some(3));
        scale.fractional(150);
        assert_eq!(scale.factor(), 1.25);
        assert!(scale.is_fractional());
    }

    #[test]
    fn the_surfaces_own_preference_outranks_the_outputs_it_is_on() {
        let mut scale = Scale::default();
        scale.outputs(Some(1));
        scale.preferred(2);
        assert_eq!(scale.factor(), 2.0);
        assert_eq!(scale.buffer_scale(), 2);
    }

    #[test]
    fn a_surface_across_two_monitors_takes_the_sharper_one() {
        let mut scale = Scale::default();
        scale.outputs(Some(2));
        assert_eq!(scale.factor(), 2.0);
    }

    #[test]
    fn the_seed_gives_the_first_frame_its_scale_until_the_compositor_speaks() {
        let mut scale = Scale::default();
        scale.seed(Some(1.5));
        assert_eq!(scale.factor(), 1.5);
        scale.outputs(Some(2));
        assert_eq!(
            scale.factor(),
            2.0,
            "an output the surface entered outranks the seed"
        );
        scale.outputs(None);
        scale.fractional(150);
        assert_eq!(scale.factor(), 1.25);
    }

    #[test]
    fn a_scale_of_zero_is_refused_wherever_it_comes_from() {
        // Every conversion out of physical pixels divides by this, and a compositor is allowed to
        // answer with nonsense while a surface is being mapped.
        let mut zero = Scale::default();
        zero.fractional(0);
        assert_eq!(zero.factor(), 1.0);

        let mut seeded = Scale::default();
        seeded.seed(Some(0.0));
        assert_eq!(seeded.factor(), 1.0);

        let mut negative = Scale::default();
        negative.preferred(-1);
        assert_eq!(negative.factor(), 1.0);
        assert_eq!(negative.buffer_scale(), 1);
    }
}
