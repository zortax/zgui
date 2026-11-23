//! A shader macro that reaches past its exception.

/// Assembles a paint shader.
pub fn assemble(snippet: &str) -> Option<String> {
    compositor_wgsl::effect(compositor_types::ShaderMode::PAINT, snippet)
}
