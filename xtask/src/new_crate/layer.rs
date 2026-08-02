//! The layer a new crate belongs to.

use std::fmt::{self, Display};
use std::str::FromStr;

/// One layer of the crate graph. Dependencies point strictly downward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Layer {
    /// Foundation: geometry, colour, allocation, atoms, bits, counters.
    L0,
    /// Contracts: the traits and vocabularies the rest of the tree agrees on.
    L1,
    /// Backends: renderers and platforms.
    L2,
    /// The document.
    L3,
    /// Engines: style, layout, text, paint.
    L4,
    /// Systems: input, scrolling, accessibility, animation, editing.
    L5,
    /// The frontend: views, macros, elements.
    L6,
    /// Runtime and tooling.
    L7,
    /// Product: the umbrella crate and the component library.
    L8,
}

impl Layer {
    /// A one-line description of what the layer holds.
    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::L0 => "foundation",
            Self::L1 => "contracts",
            Self::L2 => "backends",
            Self::L3 => "document",
            Self::L4 => "engines",
            Self::L5 => "systems",
            Self::L6 => "frontend",
            Self::L7 => "runtime and tooling",
            Self::L8 => "product",
        }
    }
}

impl Display for Layer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::L0 => "L0",
            Self::L1 => "L1",
            Self::L2 => "L2",
            Self::L3 => "L3",
            Self::L4 => "L4",
            Self::L5 => "L5",
            Self::L6 => "L6",
            Self::L7 => "L7",
            Self::L8 => "L8",
        };
        formatter.write_str(name)
    }
}

impl FromStr for Layer {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "L0" => Ok(Self::L0),
            "L1" => Ok(Self::L1),
            "L2" => Ok(Self::L2),
            "L3" => Ok(Self::L3),
            "L4" => Ok(Self::L4),
            "L5" => Ok(Self::L5),
            "L6" => Ok(Self::L6),
            "L7" => Ok(Self::L7),
            "L8" => Ok(Self::L8),
            other => Err(format!("`{other}` is not a layer; write L0 through L8")),
        }
    }
}
