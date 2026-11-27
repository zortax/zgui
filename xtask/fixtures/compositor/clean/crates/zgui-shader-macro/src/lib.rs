//! A shader macro that assembles through the one implementation its exception names.

/// Assembles a paint shader.
pub fn assemble(snippet: &str) -> Option<String> {
    iceblink_wgsl::effect(iceblink::ShaderMode::PAINT, snippet)
}
