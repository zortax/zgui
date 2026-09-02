//! The shader sources, and the check that they agree with the Rust structures.
//!
//! What each module is concatenated from is decided where it is compiled, which is at build time —
//! see [`Module::source`] and [`Module::representation`].

pub mod layout;
mod precompiled;
pub mod reflect;

use crate::shader::reflect::{Reflected, reflected};

/// Which module a pipeline is built from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Module {
    /// Rounded, bordered rectangles.
    Quad,
    /// Box shadows.
    Shadow,
    /// Text decoration lines.
    Decoration,
    /// Single-channel coverage sprites.
    MonoSprite,
    /// Full-colour sprites.
    ColorSprite,
    /// Per-channel coverage sprites.
    SubpixelSprite,
    /// The copy to the surface.
    Blit,
    /// Clearing one damage rectangle.
    Clear,
    /// The separable blur chain.
    Blur,
    /// Compositing an isolated target back into the one beneath it.
    Composite,
    /// A rectangle showing a texture the renderer did not draw.
    External,
    /// Compositing a rasterised vector batch back into the target.
    Vector,
}

impl Module {
    /// Every module, which is what makes "each one compiles" a statement about all of them.
    pub const ALL: [Self; 12] = [
        Self::Quad,
        Self::Shadow,
        Self::Decoration,
        Self::MonoSprite,
        Self::ColorSprite,
        Self::SubpixelSprite,
        Self::Blit,
        Self::Clear,
        Self::Blur,
        Self::Composite,
        Self::External,
        Self::Vector,
    ];

    /// Whether this module reads the frame's globals and side tables.
    ///
    /// Three do not, and each for the same reason: what they draw is decided entirely by the
    /// scissor, the block bound beside them, or both. A module that reads none of the tables gets
    /// no bind group for them, so this is the one place the two facts are kept together.
    pub fn uses_tables(self) -> bool {
        !matches!(self, Self::Blit | Self::Clear | Self::Blur)
    }

    /// A label for the shader module, so a driver error names which one.
    pub fn label(self) -> &'static str {
        match self {
            Self::Quad => "zgui.shader.quad",
            Self::Shadow => "zgui.shader.shadow",
            Self::Decoration => "zgui.shader.decoration",
            Self::MonoSprite => "zgui.shader.mono_sprite",
            Self::ColorSprite => "zgui.shader.color_sprite",
            Self::SubpixelSprite => "zgui.shader.subpixel_sprite",
            Self::Blit => "zgui.shader.blit",
            Self::Clear => "zgui.shader.clear",
            Self::Blur => "zgui.shader.blur",
            Self::Composite => "zgui.shader.composite",
            Self::External => "zgui.shader.external",
            Self::Vector => "zgui.shader.vector",
        }
    }
}

/// The structures this module declares, and what Rust says each of them is.
///
/// Every instance is copied into a buffer as bytes, so a field inserted on one side and not the
/// other shifts everything after it on that side alone — a rendering artefact with no error
/// anywhere. This is the comparison that turns that into a failure at pipeline creation.
pub fn structures(module: Module) -> Vec<Reflected> {
    use crate::bind::globals::Globals;
    use crate::bind::tables::{GpuClip, GpuPaint, GpuSpatial, GpuStop};
    use zgui_scene::{ColorSprite, Decoration, MonoSprite, Quad, Shadow, SubpixelSprite};

    if !module.uses_tables() {
        // A module that reads no side tables declares only its own block, or nothing at all.
        return match module {
            Module::Blur => vec![reflected!(
                crate::pipeline::blur::BlurParams,
                "BlurParams",
                [extents, kernel, sampling, valid]
            )],
            _ => Vec::new(),
        };
    }
    let mut structures = vec![
        reflected!(Globals, "Globals", [viewport, gamma_ratios, text, frame]),
        reflected!(
            GpuClip,
            "Clip",
            [aabb, first, second, count, has_mask, mask]
        ),
        reflected!(
            GpuPaint,
            "Paint",
            [
                kind, gradient, space, flags, geometry, color, stop_start, stop_count, pad0, pad1
            ]
        ),
        reflected!(GpuStop, "Stop", [color, offset, pad]),
        reflected!(GpuSpatial, "Spatial", [matrix]),
    ];
    structures.push(match module {
        Module::Quad => reflected!(
            Quad,
            "Quad",
            [
                order,
                style,
                bounds,
                radii,
                border,
                fill,
                stroke,
                clip,
                transform,
                shape,
                paint_origin
            ]
        ),
        Module::Shadow => reflected!(
            Shadow,
            "Shadow",
            [
                order,
                blur,
                bounds,
                radii,
                element_bounds,
                element_radii,
                color,
                clip,
                transform,
                inset,
                shape
            ]
        ),
        Module::Decoration => reflected!(
            Decoration,
            "Decoration",
            [
                order, style, bounds, color, thickness, clip, transform, reserved
            ]
        ),
        Module::MonoSprite => reflected!(
            MonoSprite,
            "Sprite",
            [order, reserved, bounds, color, tile, clip, transform]
        ),
        Module::SubpixelSprite => reflected!(
            SubpixelSprite,
            "Sprite",
            [order, reserved, bounds, color, tile, clip, transform]
        ),
        Module::ColorSprite => reflected!(
            ColorSprite,
            "ColorSprite",
            [
                order, flags, bounds, frame, radii, tile, opacity, clip, transform
            ]
        ),
        Module::Composite => reflected!(
            crate::pipeline::composite::CompositeParams,
            "CompositeParams",
            [
                bounds,
                source,
                control,
                tint,
                matrix0,
                matrix1,
                matrix2,
                matrix3,
                matrix_offset
            ]
        ),
        Module::External => reflected!(
            crate::pipeline::external::ExternalParams,
            "ExternalParams",
            [bounds, control]
        ),
        Module::Vector => reflected!(
            crate::pipeline::vector::VectorInstance,
            "VectorInstance",
            [bounds, source, control]
        ),
        Module::Blit | Module::Clear | Module::Blur => {
            unreachable!("a module reading no side tables was answered above")
        }
    });
    structures
}

/// Checks every module's structures against its own source.
///
/// Runs where the pipelines are created, so the failure lands at startup rather than in a frame.
pub fn check_layouts() -> Result<(), String> {
    for module in Module::ALL {
        reflect::check(module.source(), &structures(module))
            .map_err(|error| format!("{}: {error}", module.label()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Module, check_layouts, structures};

    #[test]
    fn every_shader_structure_agrees_with_the_rust_one() {
        check_layouts().expect("the shader and Rust layouts must agree");
    }

    #[test]
    fn every_module_declares_the_instance_structure_it_draws() {
        for module in Module::ALL {
            let source = module.source();
            let declared = structures(module);
            if !module.uses_tables() {
                assert!(
                    declared.len() <= 1,
                    "{module:?} reads no side tables and cannot declare them"
                );
            } else {
                assert!(
                    declared.len() >= 6,
                    "{module:?} compares only {} structures",
                    declared.len()
                );
            }
            for structure in declared {
                assert!(
                    source.contains(&format!("struct {} ", structure.name)),
                    "{module:?} does not declare {}",
                    structure.name
                );
            }
        }
    }

    /// The check has to be able to fail, or "the layouts agree" is a sentence about nothing.
    #[test]
    fn a_field_inserted_on_one_side_alone_is_caught() {
        let source = Module::Quad
            .source()
            .replace("struct Quad {", "struct Quad {\n    inserted: u32,");
        let error = super::reflect::check(&source, &structures(Module::Quad))
            .expect_err("a shader with an extra field disagrees with the Rust structure");
        assert!(error.contains("Quad"), "{error}");
    }

    #[test]
    fn a_module_carries_the_shared_coverage_function_exactly_once() {
        for module in Module::ALL {
            let source = module.source();
            let definitions = source.matches("fn clip_coverage(").count();
            let expected = usize::from(module.uses_tables());
            assert_eq!(definitions, expected, "{module:?}");
        }
    }
}
