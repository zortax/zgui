//! The shading modules as a driver takes them, rather than as they were written.

use wgpu::naga;

use crate::shader::Module;

/// The representation of one module, encoded when this crate was built.
macro_rules! ir {
    ($name:literal) => {
        include_bytes!(concat!(env!("OUT_DIR"), "/", $name, ".ir"))
    };
}

/// The text of one module, concatenated when this crate was built.
macro_rules! text {
    ($name:literal) => {
        include_str!(concat!(env!("OUT_DIR"), "/", $name, ".wgsl"))
    };
}

impl Module {
    /// The whole source of this module.
    ///
    /// This is what the module *was*, kept for the comparison between what a shader declares a
    /// structure to be and what Rust declares it to be. Nothing draws from it: a pipeline is built
    /// from [`Module::representation`], which is this text already parsed.
    pub fn source(self) -> &'static str {
        match self {
            Self::Quad => text!("quad"),
            Self::Shadow => text!("shadow"),
            Self::Decoration => text!("decoration"),
            Self::MonoSprite => text!("mono_sprite"),
            Self::ColorSprite => text!("color_sprite"),
            Self::SubpixelSprite => text!("subpixel_sprite"),
            Self::Blit => text!("blit"),
            Self::Clear => text!("clear"),
            Self::Blur => text!("blur"),
            Self::Composite => text!("composite"),
            Self::External => text!("external"),
            Self::Vector => text!("vector"),
            Self::MarksDisc => text!("marks_disc"),
            Self::MarksBox => text!("marks_box"),
            Self::MarksPolyline => text!("marks_polyline"),
            Self::MarksGlyph => text!("marks_glyph"),
            Self::MarksComposite => text!("marks_composite"),
        }
    }

    /// The encoded representation of this module.
    fn encoded(self) -> &'static [u8] {
        match self {
            Self::Quad => ir!("quad"),
            Self::Shadow => ir!("shadow"),
            Self::Decoration => ir!("decoration"),
            Self::MonoSprite => ir!("mono_sprite"),
            Self::ColorSprite => ir!("color_sprite"),
            Self::SubpixelSprite => ir!("subpixel_sprite"),
            Self::Blit => ir!("blit"),
            Self::Clear => ir!("clear"),
            Self::Blur => ir!("blur"),
            Self::Composite => ir!("composite"),
            Self::External => ir!("external"),
            Self::Vector => ir!("vector"),
            Self::MarksDisc => ir!("marks_disc"),
            Self::MarksBox => ir!("marks_box"),
            Self::MarksPolyline => ir!("marks_polyline"),
            Self::MarksGlyph => ir!("marks_glyph"),
            Self::MarksComposite => ir!("marks_composite"),
        }
    }

    /// This module as a driver takes it: already parsed, already lowered.
    ///
    /// # Panics
    ///
    /// Panics if the encoded form does not decode, which means this crate's own build output was
    /// replaced by something else. There is no recovery from it that is better than saying so:
    /// the text it was encoded from describes what to draw, and a program that cannot read its own
    /// shaders draws nothing.
    pub fn representation(self) -> naga::Module {
        bincode::deserialize(self.encoded())
            .unwrap_or_else(|error| panic!("{}: {error}", self.label()))
    }
}

#[cfg(test)]
mod tests {
    use crate::shader::Module;

    /// The build compiled every module, or the build failed; this is what says so from inside the
    /// program, which is the side that has to be able to draw without a shader compiler.
    #[test]
    fn every_shader_is_precompiled() {
        for module in Module::ALL {
            let encoded = module.encoded();
            assert!(!encoded.is_empty(), "{module:?} has no representation");
            let representation = module.representation();
            assert!(
                !representation.entry_points.is_empty(),
                "{module:?} decoded to a module nothing can be drawn with"
            );
            assert!(
                !representation.types.is_empty(),
                "{module:?} decoded to a module declaring no types"
            );
        }
    }

    /// Every module passes the same validation a device runs when a pipeline is made, so a shader
    /// that parses and would not validate fails here rather than on the first frame that uses it.
    #[test]
    fn every_shader_validates() {
        use wgpu::naga::valid::{Capabilities, ValidationFlags, Validator};

        for module in Module::ALL {
            let representation = module.representation();
            Validator::new(ValidationFlags::all(), Capabilities::all())
                .validate(&representation)
                .unwrap_or_else(|error| panic!("{module:?}: {error:?}"));
        }
    }

    /// And the text it was compiled from is still the text the layout comparison reads.
    #[test]
    fn the_text_a_module_was_compiled_from_is_the_text_it_reports() {
        for module in Module::ALL {
            assert!(
                module.source().contains("fn "),
                "{module:?} reports no source at all"
            );
        }
    }
}
