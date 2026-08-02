//! The stack-compiles canary.
//!
//! This crate builds every external engine the framework depends on into a single compilation
//! unit and names one type from each, so that a version bump, a feature-set change or a
//! toolchain move that breaks the combination fails here rather than three months later inside
//! an engine crate. It ships nothing and is never published.
//!
//! It is deliberately the only workspace member allowed to name every engine at once; every
//! other crate is restricted to the engines its layer is permitted to see.

#![forbid(unsafe_code)]

/// One value from each external engine, held together in a single type.
///
/// Constructing a [`Canary`] proves that the engines link, that their shared transitive
/// dependencies resolved to compatible versions, and that the pinned feature sets are the ones
/// the framework actually needs. Nothing reads these fields; the type exists for the compiler.
pub struct Canary {
    /// A vector scene, from the vector rasteriser.
    pub scene: vello::Scene,
    /// A GPU backend selection, from the GPU abstraction.
    pub backends: wgpu::Backends,
    /// An accessibility role, from the pure-data accessibility vocabulary.
    pub role: accesskit::Role,
    /// An outer display type, from the layout engine.
    pub display: taffy::Display,
    /// A paragraph alignment, from the text layout engine.
    pub alignment: parley::Alignment,
    /// A stylesheet cascade origin, from the style engine, whose library is named `style`.
    pub origin: style::stylesheets::Origin,
    /// Default window attributes, from the windowing backend.
    pub window: winit::window::WindowAttributes,
    /// A reactive cell, from the reactive graph.
    pub signal: reactive_graph::signal::ArcRwSignal<u32>,
}

impl Canary {
    /// Builds the canary.
    ///
    /// Pure construction: no GPU adapter is requested, no window is opened and no reactive
    /// runtime is installed, so this is safe to call from a test on a headless machine.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scene: vello::Scene::new(),
            backends: wgpu::Backends::VULKAN,
            role: accesskit::Role::Button,
            display: taffy::Display::Block,
            alignment: parley::Alignment::Start,
            origin: style::stylesheets::Origin::UserAgent,
            window: winit::window::WindowAttributes::default(),
            signal: reactive_graph::signal::ArcRwSignal::new(0),
        }
    }
}

impl Default for Canary {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Canary;

    #[test]
    fn the_engine_stack_links() {
        let canary = Canary::new();
        assert_eq!(canary.display, taffy::Display::Block);
    }
}
