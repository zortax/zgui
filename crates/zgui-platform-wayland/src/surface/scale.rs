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
/// The **outputs** the surface is on are the last resort, and the rule there is the largest rather
/// than the first: a window straddling a 1× and a 2× monitor drawn at 1× is visibly soft on half
/// of itself, and drawn at 2× is merely oversampled on the other half.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scale {
    /// The fractional scale in hundred-and-twentieths, when the compositor reports one.
    fractional: Option<u32>,
    /// The whole-number scale this surface prefers, when the compositor reports one.
    preferred: Option<i32>,
    /// The largest whole-number scale among the outputs the surface is on.
    outputs: Option<i32>,
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

    /// The scale to draw at, never zero and never negative.
    pub fn factor(&self) -> f64 {
        let answer = match (self.fractional, self.preferred, self.outputs) {
            (Some(fractional), _, _) => f64::from(fractional) / 120.0,
            (None, Some(preferred), _) => f64::from(preferred),
            (None, None, Some(outputs)) => f64::from(outputs),
            (None, None, None) => 1.0,
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
        self.preferred.or(self.outputs).unwrap_or(1).max(1)
    }
}

/// Tells the compositor how to read this surface's buffer.
///
/// A fractionally scaled surface declares a buffer scale of one and is mapped to its logical
/// extent by its viewport; declaring anything else would apply the scaling twice. A surface
/// without a viewport declares the whole number instead, which is the only scaling available to
/// it.
pub fn declare(surface: &crate::surface::WaylandSurface) {
    let ladder = surface.shared().ladder;
    let scale = if ladder.is_fractional() {
        1
    } else {
        ladder.buffer_scale()
    };
    surface.wl_surface().set_buffer_scale(scale.max(1));
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
    fn a_scale_of_zero_is_refused_wherever_it_comes_from() {
        // Every conversion out of physical pixels divides by this, and a compositor is allowed to
        // answer with nonsense while a surface is being mapped.
        let mut zero = Scale::default();
        zero.fractional(0);
        assert_eq!(zero.factor(), 1.0);

        let mut negative = Scale::default();
        negative.preferred(-1);
        assert_eq!(negative.factor(), 1.0);
        assert_eq!(negative.buffer_scale(), 1);
    }
}
