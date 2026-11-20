//! A declared shader's program, as the `shader!` macro compiled it.

use zgui_paint::content::shader::{ShaderField, ShaderMode};

/// One application shader, as its own build left it.
///
/// Everything here is `'static`, because the `shader!` macro emits it into the application's
/// binary.
#[derive(Clone, Copy, Debug)]
pub struct EffectProgram {
    /// What the shader does, which decides the function it defines.
    pub mode: ShaderMode,
    /// A label, so a driver error names the shader that produced it.
    pub label: &'static str,
    /// The translation unit a renderer assembles from `source`, compiled to naga IR and encoded
    /// with bincode.
    pub prebuilt: &'static [u8],
    /// The application's own text.
    pub source: &'static str,
    /// The layout of the shader's `Params` structure, as Rust has it.
    pub params: ParamsLayout,
    /// The layout of the shader's `Params` structure, as the compiled shader declares it.
    pub shader_params: ParamsLayout,
}

impl EffectProgram {
    /// Compares the two layouts of the shader's parameters.
    ///
    /// A shader whose Rust side declares no parameters agrees with any shader declaration.
    ///
    /// # Errors
    ///
    /// Returns the first difference: the sizes, the number of members, a member's name, or a
    /// member's offset and size.
    pub fn check_params(&self) -> Result<(), String> {
        let (rust, shader) = (&self.params, &self.shader_params);
        if rust.size == 0 && rust.fields.is_empty() {
            return Ok(());
        }
        let label = self.label;
        if rust.size > zgui_paint::content::shader::MAX_PARAMS_BYTES {
            return Err(format!(
                "{label}: parameters are {} bytes and at most {} fit in the block",
                rust.size,
                zgui_paint::content::shader::MAX_PARAMS_BYTES
            ));
        }
        if shader.size != rust.size {
            return Err(format!(
                "{label}: `Params` is {} bytes in the shader and {} in Rust",
                shader.size, rust.size
            ));
        }
        if shader.fields.len() != rust.fields.len() {
            return Err(format!(
                "{label}: `Params` has {} members in the shader and {} in Rust",
                shader.fields.len(),
                rust.fields.len()
            ));
        }
        for (shader, rust) in shader.fields.iter().zip(rust.fields) {
            if shader.name != rust.name {
                return Err(format!(
                    "{label}: `Params` member {} is `{}` in the shader and `{}` in Rust",
                    rust.offset, shader.name, rust.name
                ));
            }
            if shader.offset != rust.offset || shader.size != rust.size {
                return Err(format!(
                    "{label}: `Params.{}` is {} bytes at offset {} in the shader and {} bytes at \
                     offset {} in Rust",
                    rust.name, shader.size, shader.offset, rust.size, rust.offset
                ));
            }
        }
        Ok(())
    }
}

/// The layout of a shader's parameter structure: its size and its fields in declaration order.
#[derive(Clone, Copy, Debug)]
pub struct ParamsLayout {
    /// How many bytes the structure occupies.
    pub size: usize,
    /// Its fields, in declaration order.
    pub fields: &'static [ParamsField],
}

/// One field of a shader's parameters, as its name, offset and the bytes up to the next field.
pub type ParamsField = ShaderField;

impl ParamsLayout {
    /// The layout of a shader that declares no parameters.
    pub const EMPTY: Self = Self {
        size: 0,
        fields: &[],
    };
}
