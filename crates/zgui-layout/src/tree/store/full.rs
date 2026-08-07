//! The single cached answer that performed a box's full layout.

use taffy::{AvailableSpace, LayoutInput, LayoutOutput, RequestedAxis, RunMode};

/// The input fields Taffy's final-layout cache keys on, encoded without floats' equality traps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Probe {
    dimensions: [Dimension; 2],
    parent: [OptionBits; 2],
    axis: RequestedAxis,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dimension {
    Known(u32),
    Definite(u32),
    MinContent,
    MaxContent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OptionBits {
    None,
    Some(u32),
}

impl Probe {
    fn of(input: &LayoutInput) -> Self {
        let dimension = |known: Option<f32>, available: AvailableSpace| match known {
            Some(value) => Dimension::Known(value.to_bits()),
            None => match available {
                AvailableSpace::Definite(value) => Dimension::Definite(value.to_bits()),
                AvailableSpace::MinContent => Dimension::MinContent,
                AvailableSpace::MaxContent => Dimension::MaxContent,
            },
        };
        let optional = |value: Option<f32>| match value {
            Some(value) => OptionBits::Some(value.to_bits()),
            None => OptionBits::None,
        };
        Self {
            dimensions: [
                dimension(input.known_dimensions.width, input.available_space.width),
                dimension(input.known_dimensions.height, input.available_space.height),
            ],
            parent: [
                optional(input.parent_size.width),
                optional(input.parent_size.height),
            ],
            axis: input.axis,
        }
    }
}

/// One final-layout answer. Size-only answers live in the wider measurement cache.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FullLayout {
    held: Option<(Probe, LayoutOutput)>,
}

impl FullLayout {
    pub(crate) const fn new() -> Self {
        Self { held: None }
    }

    pub(crate) fn get(&self, input: &LayoutInput) -> Option<LayoutOutput> {
        if input.run_mode != RunMode::PerformLayout {
            return None;
        }
        let (probe, output) = self.held?;
        (probe == Probe::of(input)).then_some(output)
    }

    pub(crate) fn store(&mut self, input: &LayoutInput, output: LayoutOutput) {
        if input.run_mode == RunMode::PerformLayout {
            self.held = Some((Probe::of(input), output));
        }
    }

    pub(crate) fn clear(&mut self) {
        self.held = None;
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.held.is_none()
    }
}

impl Default for FullLayout {
    fn default() -> Self {
        Self::new()
    }
}
