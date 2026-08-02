//! Which ramp paints the text at a line box, and which box it came from.
//!
//! `background-image` is not an inherited property, and a line box belongs to an anonymous inline
//! root generated *below* the element that declared one. So a box that asked for its background to
//! paint its text has a ramp its own line boxes cannot see: they inherit the custom property that
//! made the request — custom properties do inherit — and inherit no ramp to answer it with, and the
//! text comes out in `color` under a declaration that reads as though it should not.
//!
//! This is the same propagation `text-decoration` needs and for the same reason, so it is done the
//! same way: contributed on the way into a box, withdrawn on the way out, and read at the line box
//! rather than off the line box's own style.
//!
//! # Where propagation stops
//!
//! At the same places a decoration's does — a float, an out-of-flow box, an atomic inline-level box
//! — because those are formatting contexts of their own and their text is not the text the ramp was
//! asked to cut. An `inline-block` inside a gradient heading paints its own text in its own colour,
//! which is what `background-clip: text` does in a browser.

use zgui_css::ComputedStyle;

use crate::lower::background::GradientSpec;
use crate::walk::decorate::interrupts_propagation;

/// The ramp painting text as the walk descends.
///
/// A stack with a *floor*, exactly as the decoration list is: entering a box that interrupts
/// propagation raises the floor, hiding what ancestors contributed without discarding it, and
/// leaving the box puts it back.
#[derive(Debug, Default)]
pub struct TextFills {
    /// Every ramp contributed by a box still on the walk, outermost first.
    contributed: Vec<GradientSpec>,
    /// The index the visible list starts at.
    floor: usize,
    /// What to restore on the way out: the length and the floor as they were on the way in.
    frames: Vec<(usize, usize)>,
}

impl TextFills {
    /// A walk that has entered nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enters a box, contributing whatever ramp it asked to paint its text with.
    pub fn enter(&mut self, style: &ComputedStyle, fill: Option<&GradientSpec>) {
        self.frames.push((self.contributed.len(), self.floor));
        if interrupts_propagation(style) {
            self.floor = self.contributed.len();
        }
        if let Some(spec) = fill {
            self.contributed.push(spec.clone());
        }
    }

    /// Leaves the box most recently entered.
    ///
    /// # Panics
    ///
    /// Panics if more boxes are left than were entered, which would mean the walk's enter and leave
    /// calls are not paired.
    pub fn leave(&mut self) {
        let (length, floor) = self
            .frames
            .pop()
            .expect("every leave follows its own enter");
        self.contributed.truncate(length);
        self.floor = floor;
    }

    /// The ramp a line box here is painted with, which is the innermost one still in force.
    ///
    /// Innermost, not outermost: a heading inside a gradient section paints its own text with its
    /// own ramp, the same way a nested `color` wins.
    pub fn in_force(&self) -> Option<&GradientSpec> {
        self.contributed.get(self.floor..)?.last()
    }
}

/// A fingerprint of the ramp in force, for deciding whether a recorded fragment may be replayed.
///
/// The ramp comes from a box *above* the fragment, so nothing in the fragment's own record moves
/// when it changes: without this, editing the gradient on a heading replays every line inside it
/// exactly as it was.
pub fn signature(fill: Option<&GradientSpec>) -> u64 {
    let mut hash = zgui_scene::ContentHash::new();
    match fill {
        None => hash = hash.u32(0),
        Some(spec) => {
            hash = hash.u32(1).u32(spec.stops.len() as u32);
            hash = hash.u32(u32::from(spec.repeating));
            for stop in &spec.stops {
                let [r, g, b, a] = stop.color.to_premultiplied_srgb();
                hash = hash.f32(r).f32(g).f32(b).f32(a).f32(
                    stop.position
                        .as_ref()
                        .map_or(f32::NAN, |offset| offset.fraction(zgui_geom::CssPx(1.0))),
                );
            }
        }
    }
    hash.finish()
}

#[cfg(test)]
mod tests {
    use super::TextFills;

    #[test]
    fn a_walk_that_entered_nothing_has_no_ramp_in_force() {
        assert!(TextFills::new().in_force().is_none());
    }
}
