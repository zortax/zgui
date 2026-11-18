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
    /// The discs of a mark.
    MarksDisc,
    /// The boxes of a mark.
    MarksBox,
    /// The polylines of a mark.
    MarksPolyline,
    /// The glyphs of a mark.
    MarksGlyph,
    /// Painting a union mark through its bin.
    MarksComposite,
}

impl Module {
    /// Every module, which is what makes "each one compiles" a statement about all of them.
    pub const ALL: [Self; 17] = [
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
        Self::MarksDisc,
        Self::MarksBox,
        Self::MarksPolyline,
        Self::MarksGlyph,
        Self::MarksComposite,
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
            Self::MarksDisc => "zgui.shader.marks_disc",
            Self::MarksBox => "zgui.shader.marks_box",
            Self::MarksPolyline => "zgui.shader.marks_polyline",
            Self::MarksGlyph => "zgui.shader.marks_glyph",
            Self::MarksComposite => "zgui.shader.marks_composite",
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
    use zgui_scene::{
        ColorSprite, Decoration, MarkBox, MarkItem, MonoSprite, Quad, Shadow, SubpixelSprite,
    };

    if !module.uses_tables() {
        // A module that reads no side tables declares only its own block, or nothing at all.
        return match module {
            Module::Blur => vec![reflected!(
                crate::pipeline::blur::BlurParams,
                "BlurParams",
                [extents, kernel, sampling, valid, origins]
            )],
            _ => Vec::new(),
        };
    }
    let mut structures = vec![
        reflected!(
            Globals,
            "Globals",
            [viewport, gamma_ratios, text, frame, origin]
        ),
        reflected!(
            GpuClip,
            "Clip",
            [
                aabb, first, second, kept_aabb, count, space, kept_tests, pad
            ]
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
    if matches!(
        module,
        Module::MarksDisc
            | Module::MarksBox
            | Module::MarksPolyline
            | Module::MarksGlyph
            | Module::MarksComposite
    ) {
        structures.push(reflected!(
            MarkItem,
            "MarkItem",
            [
                order,
                flags,
                bounds,
                paint,
                clip,
                transform,
                paint_origin,
                origin,
                discs,
                boxes,
                vertices,
                half_width,
                axes,
                glyphs,
                tiles,
                texture,
                first
            ]
        ));
        structures.push(reflected!(
            crate::pipeline::marks::MarkDraw,
            "MarkDraw",
            [position, page, shift, region]
        ));
        if module == Module::MarksBox {
            structures.push(reflected!(MarkBox, "MarkBox", [rect, radii, shape]));
        }
        return structures;
    }
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
                shape_bounds,
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
                matrix_offset,
                origin
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
        Module::MarksDisc
        | Module::MarksBox
        | Module::MarksPolyline
        | Module::MarksGlyph
        | Module::MarksComposite => {
            unreachable!("a mark module was answered above")
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
    /// What one scalar of a record is, as a loader has to produce it.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Word {
        /// Read as it is.
        Uint,
        /// Reinterpreted as a signed integer.
        Sint,
        /// Reinterpreted as a float.
        Float,
    }

    /// The scalars of `ty` in declaration order, or why it is not a run of four-byte scalars.
    fn scalars(
        module: &naga::Module,
        ty: naga::Handle<naga::Type>,
        into: &mut Vec<Word>,
    ) -> Result<(), String> {
        let word = |scalar: naga::Scalar| match (scalar.kind, scalar.width) {
            (naga::ScalarKind::Uint, 4) => Ok(Word::Uint),
            (naga::ScalarKind::Sint, 4) => Ok(Word::Sint),
            (naga::ScalarKind::Float, 4) => Ok(Word::Float),
            other => Err(format!("a scalar that is not four bytes: {other:?}")),
        };
        match &module.types[ty].inner {
            naga::TypeInner::Scalar(scalar) => into.push(word(*scalar)?),
            naga::TypeInner::Vector { size, scalar } => {
                into.extend(std::iter::repeat_n(word(*scalar)?, *size as usize));
            }
            naga::TypeInner::Matrix {
                columns,
                rows,
                scalar,
            } => {
                // A column of three is padded to four, and a loader has no texel for the padding.
                if *rows != naga::VectorSize::Quad {
                    return Err("a matrix whose columns are padded".to_owned());
                }
                into.extend(std::iter::repeat_n(
                    word(*scalar)?,
                    *columns as usize * *rows as usize,
                ));
            }
            naga::TypeInner::Struct { members, span } => {
                let start = into.len();
                for member in members {
                    if member.offset as usize != (into.len() - start) * 4 {
                        return Err(format!("padding before `{:?}`", member.name));
                    }
                    scalars(module, member.ty, into)?;
                }
                if *span as usize != (into.len() - start) * 4 {
                    return Err("padding at the end of a structure".to_owned());
                }
            }
            other => return Err(format!("a type a loader cannot produce: {other:?}")),
        }
        Ok(())
    }

    /// The texel a `table_texel(...)` call that starts at `at` names, past the loader's base.
    fn texel_of(text: &str, at: usize) -> u32 {
        let argument = &text[at + "table_texel(".len()..];
        let argument = &argument[..argument.find(')').expect("the call closes")];
        argument
            .rsplit_once("+ ")
            .and_then(|(_, texel)| texel.trim_end_matches('u').parse().ok())
            .unwrap_or(0)
    }

    /// The words a loader's `return` reads, in the order it reads them, and what it makes of each.
    fn words_read(body: &str) -> Vec<(u32, Word)> {
        // Each `let name = textureLoad(...)` is one texel, already floats under `texel_f32`.
        let mut texels: Vec<(&str, u32, bool)> = Vec::new();
        for line in body.lines().map(str::trim) {
            let Some(rest) = line.strip_prefix("let ") else {
                continue;
            };
            let Some((name, value)) = rest.split_once(" = ") else {
                continue;
            };
            if let Some(at) = value.find("table_texel(") {
                texels.push((name, texel_of(value, at), value.starts_with("texel_f32(")));
            }
        }
        let returned = &body[body.find("return ").expect("a loader returns its record")..];
        let mut read: Vec<(usize, u32, Word)> = Vec::new();
        // A texel loaded where it is passed is four words: floats under `texel_f32`, else as read.
        for (at, _) in returned.match_indices("textureLoad(") {
            let floats = returned[..at].ends_with("texel_f32(");
            let (word, close) = if floats {
                (Word::Float, ", 0))")
            } else {
                (Word::Uint, ", 0)")
            };
            let call = at
                + returned[at..]
                    .find("table_texel(")
                    .expect("a load names a texel");
            let texel = texel_of(returned, call);
            // All four channels, unless a swizzle after the call keeps some of them.
            let after = &returned[call..];
            let after = &after[after.find(close).expect("the load closes") + close.len()..];
            let kept: Vec<u32> = match after.strip_prefix('.') {
                Some(swizzle) => swizzle
                    .chars()
                    .map_while(|letter| "xyzw".find(letter).map(|channel| channel as u32))
                    .collect(),
                None => (0..4).collect(),
            };
            read.extend(
                kept.into_iter()
                    .map(|channel| (at, texel * 4 + channel, word)),
            );
        }
        for &(name, texel, floats) in &texels {
            for (component, letter) in ["x", "y", "z", "w"].into_iter().enumerate() {
                let field = format!("{name}.{letter}");
                for (at, _) in returned.match_indices(&field) {
                    let whole_name = returned[..at]
                        .chars()
                        .next_back()
                        .is_none_or(|before| !before.is_alphanumeric() && before != '_');
                    let whole_field = returned[at + field.len()..]
                        .chars()
                        .next()
                        .is_none_or(|after| !after.is_alphanumeric() && after != '_');
                    if !whole_name || !whole_field {
                        continue;
                    }
                    let word = if returned[..at].ends_with("bitcast<f32>(") {
                        Word::Float
                    } else if returned[..at].ends_with("bitcast<i32>(") {
                        Word::Sint
                    } else if floats {
                        Word::Float
                    } else {
                        Word::Uint
                    };
                    read.push((at, texel * 4 + component as u32, word));
                }
            }
        }
        // Stable, so the four words of one converted texel keep their order.
        read.sort_by_key(|(at, ..)| *at);
        read.into_iter()
            .map(|(_, index, word)| (index, word))
            .collect()
    }

    /// Checks every `load_*` function of `source` against the type it returns.
    ///
    /// A record reaches the shader as texels and a loader names each field by texel and channel.
    /// The comparison above ties the Rust structure to the shader's declaration of it. This ties
    /// the declaration to the loader: every scalar is read once, in order, from the word it is
    /// stored at, and is reinterpreted as the type the field has.
    fn check_loaders(source: &str, module: &naga::Module) -> Result<usize, String> {
        let mut checked = 0;
        let loaders = source
            .match_indices("\nfn load_")
            .chain(source.match_indices("\nfn clip_rounded("));
        for (at, _) in loaders {
            let text = &source[at + 1..];
            let text = &text[..text.find("\n}").expect("a function closes")];
            let name = &text["fn ".len()..text.find('(').expect("a function has parameters")];
            let (_, function) = module
                .functions
                .iter()
                .find(|(_, function)| function.name.as_deref() == Some(name))
                .ok_or_else(|| format!("`{name}` is not in the compiled module"))?;
            let result = function
                .result
                .as_ref()
                .ok_or_else(|| format!("`{name}` returns nothing"))?;
            let mut expected = Vec::new();
            scalars(module, result.ty, &mut expected).map_err(|why| format!("`{name}`: {why}"))?;
            let read = words_read(text);
            let wanted: Vec<(u32, Word)> = expected
                .iter()
                .enumerate()
                .map(|(index, word)| (index as u32, *word))
                .collect();
            if read != wanted {
                let first = read
                    .iter()
                    .zip(&wanted)
                    .position(|(read, wanted)| read != wanted)
                    .unwrap_or(read.len().min(wanted.len()));
                return Err(format!(
                    "`{name}` reads {} words and its record has {}; they first differ at word \
                     {first}: read {:?}, declared {:?}",
                    read.len(),
                    wanted.len(),
                    read.get(first),
                    wanted.get(first),
                ));
            }
            // The stride the loader steps by is the record rounded up to whole texels.
            if let Some(stride) = text
                .lines()
                .find_map(|line| line.trim().strip_prefix("let base = "))
                .and_then(|base| base.rsplit_once("* "))
                .and_then(|(_, stride)| {
                    stride
                        .trim_end_matches(';')
                        .trim_end_matches('u')
                        .parse()
                        .ok()
                })
            {
                let texels: usize = expected.len().div_ceil(4);
                if texels != stride {
                    return Err(format!(
                        "`{name}` steps by {stride} texels and its record is {texels}"
                    ));
                }
            }
            checked += 1;
        }
        Ok(checked)
    }

    #[test]
    fn every_loader_reads_each_field_from_the_word_it_is_stored_at() {
        let mut checked = 0;
        for module in Module::ALL {
            checked += check_loaders(module.source(), &module.representation())
                .unwrap_or_else(|error| panic!("{}: {error}", module.label()));
        }
        assert!(checked >= 3 * 14, "only {checked} loaders were compared");
    }

    /// A clip is read a field at a time, so each accessor names its texel by hand.
    #[test]
    fn the_clip_accessors_name_the_texels_the_fields_are_stored_at() {
        use crate::bind::tables::GpuClip;
        use core::mem::{offset_of, size_of};

        let source = Module::Quad.source();
        let texel = |offset: usize| {
            assert_eq!(offset % 16, 0, "a clip field starts inside a texel");
            offset / 16
        };
        let names = |function: &str, expected: String| {
            let at = source
                .find(&format!("fn {function}("))
                .unwrap_or_else(|| panic!("the shader declares no `{function}`"));
            let body = &source[at..at + source[at..].find("\n}").expect("a function closes")];
            assert!(
                body.contains(&expected),
                "`{function}` does not read `{expected}`"
            );
        };
        let stride = format!("const CLIP_TEXELS: u32 = {}u;", size_of::<GpuClip>() / 16);
        assert!(
            source.contains(&stride),
            "the shader does not declare `{stride}`"
        );
        names(
            "clip_aabb",
            format!("id * CLIP_TEXELS + {}u)", texel(offset_of!(GpuClip, aabb))),
        );
        names(
            "clip_rounded",
            format!(
                "id * CLIP_TEXELS + {}u + which * {}u;",
                texel(offset_of!(GpuClip, first)),
                texel(offset_of!(GpuClip, second)) - texel(offset_of!(GpuClip, first)),
            ),
        );
        names(
            "clip_kept_aabb",
            format!(
                "id * CLIP_TEXELS + {}u)",
                texel(offset_of!(GpuClip, kept_aabb))
            ),
        );
        // The three scalars share the last texel, one channel each.
        let last = texel(offset_of!(GpuClip, count));
        for (function, offset, channel) in [
            ("clip_count", offset_of!(GpuClip, count), 'x'),
            ("clip_space", offset_of!(GpuClip, space), 'y'),
            ("clip_kept_tests", offset_of!(GpuClip, kept_tests), 'z'),
        ] {
            assert_eq!(
                offset,
                last * 16 + "xyzw".find(channel).expect("a channel") * 4
            );
            names(
                function,
                format!("id * CLIP_TEXELS + {last}u), 0).{channel};"),
            );
        }
    }

    /// The check has to be able to fail.
    #[test]
    fn a_loader_that_swaps_two_fields_is_caught() {
        let source = Module::Quad.source();
        let swapped = source.replacen("t0.x, t0.y, t0.z, t0.w,", "t0.y, t0.x, t0.z, t0.w,", 1);
        assert_ne!(source, swapped, "the fixture no longer matches the loader");
        let error = check_loaders(&swapped, &Module::Quad.representation())
            .expect_err("a loader that reads two words in the wrong order disagrees");
        assert!(error.contains("load_paint"), "{error}");
    }

    #[test]
    fn a_loader_that_forgets_a_bitcast_is_caught() {
        let source = Module::Quad.source();
        let bare = source.replacen(
            "let g = texel_f32(textureLoad(paints, table_texel(base + 1u), 0));",
            "let g = textureLoad(paints, table_texel(base + 1u), 0);",
            1,
        );
        assert_ne!(source, bare, "the fixture no longer matches the loader");
        let error = check_loaders(&bare, &Module::Quad.representation())
            .expect_err("a float field read as an integer disagrees");
        assert!(error.contains("load_paint"), "{error}");
    }
}
