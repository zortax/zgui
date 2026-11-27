//! A shader macro that reaches past its exception.

/// Assembles a paint shader.
pub fn assemble(snippet: &str) -> Option<String> {
    iceblink_wgsl::effect(iceblink::ShaderMode::PAINT, snippet)
}
