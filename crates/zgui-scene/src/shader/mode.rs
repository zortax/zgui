//! What an application's effect does, and what it reads.
//!
//! The vocabulary rather than the shading: which function an effect is, and what it reads that
//! changes on its own. Both are things the paint stage decides with — whether a rectangle replaces
//! a background or covers it, and whether the element that carries it has to be redrawn every
//! refresh — so both live here rather than beside the text.

/// What an effect does, which decides where its rectangle goes and what fills it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ShaderMode {
    /// The effect shades the colour, and the framework applies the box and the clip.
    ///
    /// `fn shade(in: ShaderInput, params: Params) -> vec4<f32>`, returning a premultiplied,
    /// gamma-encoded colour.
    Paint,
    /// The effect shapes the box, and the framework fills it with the ordinary paints.
    ///
    /// `fn coverage(in: ShaderInput, params: Params) -> f32`, returning coverage from zero to one.
    /// A gradient, an image and a border colour all keep working, because the effect decides only
    /// which pixels are inside.
    Coverage,
    /// The effect reads what was drawn beneath it and writes what replaces it.
    ///
    /// `fn apply(in: ShaderInput, params: Params, beneath: texture_2d<f32>, beneath_sampler:
    /// sampler, region: FilterSource) -> vec4<f32>`, returning a premultiplied colour. It reads
    /// the content through [`source_at`], which clamps to the region it is allowed to sample.
    ///
    /// Unlike the other two this is not a rectangle in the display list: it is an entry in a
    /// group's filter chain, so the content it reads is already in a target of its own. That is
    /// also what it costs — the same target a `blur()` or a `backdrop-filter` costs.
    Filter,
}

impl ShaderMode {
    /// Every mode.
    pub const ALL: [Self; 3] = [Self::Paint, Self::Coverage, Self::Filter];

    /// The name this mode is written under, in a declaration and in a style sheet.
    pub fn name(self) -> &'static str {
        match self {
            Self::Paint => "Paint",
            Self::Coverage => "Coverage",
            Self::Filter => "Filter",
        }
    }

    /// Whether the effect is a rectangle in the display list rather than a step of a filter chain.
    pub fn is_primitive(self) -> bool {
        matches!(self, Self::Paint | Self::Coverage)
    }

    /// The mode `name` names.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.name() == name)
    }
}

/// What an effect reads that changes on its own, and therefore what has to invalidate it.
///
/// An effect writes no fragment of its own accord, so nothing damages its rectangle unless it is
/// asked to. Declaring a read is what asks: it is the difference between an effect that costs what
/// a background costs and one that repaints every refresh for as long as it is on screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShaderReads {
    /// Whether the effect reads the clock, and therefore animates.
    pub time: bool,
    /// Whether the effect reads the pointer, and therefore repaints when it moves.
    pub pointer: bool,
}

impl ShaderReads {
    /// An effect that reads neither the clock nor the pointer.
    pub const NOTHING: Self = Self {
        time: false,
        pointer: false,
    };

    /// The two names a declaration may write.
    pub const NAMES: [&'static str; 2] = ["Time", "Pointer"];

    /// The same set, also reading whatever `name` names.
    pub fn with(mut self, name: &str) -> Option<Self> {
        match name {
            "Time" => self.time = true,
            "Pointer" => self.pointer = true,
            _ => return None,
        }
        Some(self)
    }

    /// Whether anything here makes the effect repaint without the document changing.
    pub fn is_nothing(self) -> bool {
        self == Self::NOTHING
    }
}

#[cfg(test)]
mod tests {
    use super::{ShaderMode, ShaderReads};

    #[test]
    fn every_mode_round_trips_through_its_name() {
        for mode in ShaderMode::ALL {
            assert_eq!(ShaderMode::from_name(mode.name()), Some(mode));
        }
    }

    #[test]
    fn a_mode_that_does_not_exist_names_nothing() {
        assert_eq!(ShaderMode::from_name("Refraction"), None);
    }

    #[test]
    fn exactly_the_two_rectangle_modes_are_primitives() {
        let primitives: Vec<ShaderMode> = ShaderMode::ALL
            .into_iter()
            .filter(|mode| mode.is_primitive())
            .collect();
        assert_eq!(primitives, vec![ShaderMode::Paint, ShaderMode::Coverage]);
    }

    #[test]
    fn every_declarable_read_is_accepted_and_nothing_else_is() {
        for name in ShaderReads::NAMES {
            let reads = ShaderReads::NOTHING
                .with(name)
                .expect("a declarable read is accepted");
            assert!(!reads.is_nothing(), "{name} changed nothing");
        }
        assert_eq!(ShaderReads::NOTHING.with("Backdrop"), None);
    }
}
