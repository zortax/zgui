//! A shader macro that assembles through the one implementation its exception names.

/// Assembles a paint shader.
pub fn assemble(snippet: &str) -> Option<String> {
    compositor_wgsl::effect(compositor_types::ShaderMode::PAINT, snippet)
}
