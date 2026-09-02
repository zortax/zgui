//! The shading vocabulary as text, and the rules for assembling a translation unit from it.
//!
//! Nothing here holds a device or names a graphics API. What lives here is the source every
//! pipeline is built from, in one copy, because two copies of it is the one defect the
//! shader-reflection check cannot catch: a helper edited on one side and not the other compiles on
//! both and draws two different pictures.
//!
//! Two kinds of translation unit are assembled from these pieces.
//!
//! The framework's own modules are named by [`MODULES`] and built by [`module`]. That happens once
//! per build of the wgpu backend, and a launch loads the result.
//!
//! An application's effect is built by [`effect`]: the shared vocabulary, the effect prelude, the
//! application's own function, and the epilogue for the mode it was written for. That happens once
//! per build of the application, inside the `shader!` macro.
//!
//! # Why concatenation and not imports
//!
//! A shading module is one translation unit. A function defined once and used by seven pipelines
//! is one piece of text compiled seven times, which is what makes a clip on a quad, a clip on a
//! glyph and a clip on an application's effect provably the same clip.

#![forbid(unsafe_code)]

mod mode;
mod part;

pub use crate::mode::{entry, epilogue, fragment_entry, vertex_entry};
pub use crate::part::{PART_NAMES, part};
pub use zgui_scene::{ShaderMode, ShaderReads};

/// The files each framework module is built from, in order, under the name it is known by.
pub const MODULES: &[(&str, &[&str])] = &[
    ("quad", &["common", "sdf", "paint", "quad"]),
    ("shadow", &["common", "sdf", "paint", "shadow"]),
    ("decoration", &["common", "sdf", "paint", "decoration"]),
    (
        "mono_sprite",
        &["common", "sdf", "paint", "text", "sprite", "sprite_mono"],
    ),
    (
        "color_sprite",
        &["common", "sdf", "paint", "text", "sprite", "sprite_color"],
    ),
    (
        "subpixel_sprite",
        &["common", "sdf", "paint", "text", "sprite", "subpixel"],
    ),
    // The copy reads one texture and nothing else, so it shares none of the above and its
    // bind-group layout is its own.
    ("blit", &["blit"]),
    // A clear draws a colour and reads nothing at all.
    ("clear", &["clear"]),
    // The blur chain reads one texture through one block; clips and paints belong to the composite
    // that follows it, not to the filtering itself.
    ("blur", &["blur"]),
    ("composite", &["common", "sdf", "composite"]),
    ("external", &["common", "sdf", "external"]),
    ("vector", &["common", "sdf", "vector"]),
];

/// What has to precede every other item of a module that blends against a second colour output.
///
/// An extension is declared before anything else in a translation unit, which is why it is named
/// here rather than at the top of the file that needs it.
pub const PRELUDES: &[(&str, &str)] = &[("subpixel_sprite", "enable dual_source_blending;")];

/// How many bytes of parameters an effect may declare.
///
/// Four vectors of four floats. The block is bound beside the draw rather than read per instance,
/// so the limit buys a fixed uniform stride and costs an effect nothing it would otherwise have:
/// anything larger than this is a texture rather than a parameter.
pub const MAX_PARAMS_BYTES: usize = 64;

/// The parts a rectangle effect is assembled from, before its own text.
///
/// A filter is assembled from [`EFFECT_PRELUDE`] alone. It binds the content it reads at group
/// zero, where this vocabulary binds the frame's own tables, so the two cannot be in one
/// translation unit — which is the same reason the blur chain shares none of it.
pub const EFFECT_VOCABULARY: [&str; 4] = ["common", "sdf", "paint", "effect"];

/// The part every application effect is assembled from, whatever it does.
pub const EFFECT_PRELUDE: &str = "effect";

/// The whole source of the framework module `name`, or `None` when there is no such module.
pub fn module(name: &str) -> Option<String> {
    let parts = MODULES
        .iter()
        .find(|(module, _)| *module == name)
        .map(|(_, parts)| *parts)?;
    let mut pieces: Vec<&str> = PRELUDES
        .iter()
        .filter(|(module, _)| *module == name)
        .map(|(_, text)| *text)
        .collect();
    pieces.extend(parts.iter().map(|piece| part(piece)));
    Some(pieces.join("\n"))
}

/// The whole source of an application effect written for `mode`.
///
/// `snippet` is the application's own text. It declares `struct Params` and the one function the
/// mode calls, and it is placed between the vocabulary it reads and the epilogue that calls it, so
/// the unit reads in declaration order from top to bottom.
pub fn effect(mode: ShaderMode, snippet: &str) -> String {
    let vocabulary: &[&str] = if mode.is_primitive() {
        &EFFECT_VOCABULARY
    } else {
        std::slice::from_ref(&EFFECT_PRELUDE)
    };
    let mut pieces: Vec<&str> = vocabulary.iter().map(|piece| part(piece)).collect();
    // The epilogue's block has a `Params` member whatever the effect does, so an effect that
    // declares none still needs the type to exist. Supplying it here is what lets such an effect
    // be written as the one function it is, rather than as one function and an empty structure.
    if !declares_params(snippet) {
        pieces.push(EMPTY_PARAMS);
    }
    pieces.push(snippet);
    pieces.extend(crate::epilogue(mode).iter().map(|piece| part(piece)));
    pieces.join("\n")
}

/// The parameters of an effect that declares none.
const EMPTY_PARAMS: &str = "struct Params {\n    unused: vec4<f32>,\n}\n";

/// Whether `snippet` declares its own parameters.
fn declares_params(snippet: &str) -> bool {
    snippet.match_indices("struct Params").any(|(at, matched)| {
        // `struct ParamsExtra` is a different type, and `_struct Params` is not a declaration.
        let before = snippet[..at].chars().next_back();
        let after = snippet[at + matched.len()..].chars().next();
        before.is_none_or(|char| !char.is_alphanumeric() && char != '_')
            && after.is_none_or(|char| char.is_whitespace() || char == '{')
    })
}

#[cfg(test)]
mod tests {
    use super::{MODULES, ShaderMode, effect, module, part};

    #[test]
    fn every_module_names_parts_that_exist() {
        for (name, parts) in MODULES {
            for piece in *parts {
                assert!(
                    !part(piece).is_empty(),
                    "{name} names an empty or missing part `{piece}`"
                );
            }
            assert!(module(name).is_some(), "{name} does not assemble");
        }
    }

    #[test]
    fn a_module_that_does_not_exist_assembles_to_nothing() {
        assert!(module("not-a-module").is_none());
    }

    #[test]
    fn an_effect_carries_the_snippet_between_the_vocabulary_and_the_epilogue() {
        let source = effect(ShaderMode::Paint, "// the application's own text");
        let snippet = source
            .find("// the application's own text")
            .expect("the snippet is in the source");
        let vocabulary = source
            .find("fn clip_coverage(")
            .expect("the shared clip function is in the source");
        let epilogue = source
            .find("fn fs_shaded_paint(")
            .expect("the epilogue is in the source");
        assert!(vocabulary < snippet, "the vocabulary precedes the snippet");
        assert!(snippet < epilogue, "the snippet precedes the epilogue");
    }

    #[test]
    fn a_filter_is_assembled_from_the_prelude_alone() {
        let source = effect(ShaderMode::Filter, "// the application's own text");
        assert!(
            source.contains("struct ShaderInput"),
            "the prelude is there"
        );
        assert!(source.contains("fn fs_shaded_filter("), "so is the stage");
        assert!(
            !source.contains("struct Quad"),
            "and none of the vocabulary a rectangle is drawn through"
        );
    }

    #[test]
    fn an_effect_that_declares_no_parameters_is_given_the_type_the_block_needs() {
        let source = effect(ShaderMode::Paint, "fn shade() {}");
        assert_eq!(source.matches("struct Params").count(), 1);
    }

    #[test]
    fn an_effect_that_declares_its_own_parameters_is_given_no_second_type() {
        let source = effect(ShaderMode::Paint, "struct Params {\n    amount: f32,\n}\n");
        assert_eq!(source.matches("struct Params").count(), 1);
        assert!(
            source.contains("amount: f32"),
            "the effect's own is the one kept"
        );
    }

    /// A structure whose name merely begins with `Params` is not a declaration of `Params`.
    #[test]
    fn a_similarly_named_structure_does_not_pass_for_the_parameters() {
        let source = effect(
            ShaderMode::Paint,
            "struct ParamsExtra {\n    amount: f32,\n}\n",
        );
        assert!(
            source.contains("unused: vec4<f32>"),
            "the type is still supplied"
        );
    }

    #[test]
    fn every_mode_assembles_a_unit_with_one_fragment_stage() {
        for mode in ShaderMode::ALL {
            let source = effect(mode, "");
            assert_eq!(
                source.matches("@fragment").count(),
                1,
                "{mode:?} assembles more than one fragment stage"
            );
        }
    }

    /// The shared clip function reaches every rectangle effect, so a clip on one is the clip on a
    /// background. A filter is scissored to its region rather than clipped per fragment, and binds
    /// none of the tables the function reads.
    #[test]
    fn every_rectangle_effect_carries_the_shared_clip_function_exactly_once() {
        for mode in ShaderMode::ALL
            .into_iter()
            .filter(|mode| mode.is_primitive())
        {
            let source = effect(mode, "");
            assert_eq!(source.matches("fn clip_coverage(").count(), 1, "{mode:?}");
        }
        assert_eq!(
            effect(ShaderMode::Filter, "")
                .matches("fn clip_coverage(")
                .count(),
            0
        );
    }

    /// Whatever an effect does, exactly one binding sits at each slot of each group it uses.
    #[test]
    fn no_mode_declares_two_things_at_one_binding() {
        for mode in ShaderMode::ALL {
            let source = effect(mode, "");
            let mut seen: Vec<&str> = source
                .match_indices("@group(")
                .map(|(at, _)| {
                    let tail = &source[at..];
                    &tail[..tail.find("var").unwrap_or(tail.len())]
                })
                .collect();
            let before = seen.len();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), before, "{mode:?} binds one slot twice");
        }
    }
}
