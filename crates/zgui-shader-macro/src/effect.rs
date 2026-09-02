//! The `shader!` macro: assembling one translation unit and compiling it.

use std::collections::BTreeSet;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, LitStr, Token, Type};
use zgui_wgsl::{ShaderMode, ShaderReads};

/// One `shader!` invocation, as it was written.
struct Declaration {
    /// What the effect is called.
    name: LitStr,
    /// What it does.
    mode: ShaderMode,
    /// Where the mode was written, so an error about it points at it.
    mode_span: Span,
    /// The Rust structure its parameters are.
    params: Option<Type>,
    /// How far outside its own box a filter effect reads, in CSS pixels.
    reach: f32,
    /// What it reads that changes on its own.
    reads: ShaderReads,
    /// The application's own text, and where it came from.
    source: Source,
}

/// Where an effect's text was written.
enum Source {
    /// Written in the invocation.
    Inline(LitStr),
    /// Read from a file beside the application's manifest.
    Path(LitStr),
}

impl Parse for Declaration {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut name: Option<LitStr> = None;
        let mut mode: Option<(ShaderMode, Span)> = None;
        let mut params: Option<Type> = None;
        let mut reads = ShaderReads::NOTHING;
        let mut reach = 0.0f32;
        let mut source: Option<Source> = None;
        let mut seen: BTreeSet<String> = BTreeSet::new();

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            let field = key.to_string();
            if !seen.insert(field.clone()) {
                return Err(syn::Error::new(key.span(), format!("`{field}` is written twice")));
            }
            input.parse::<Token![:]>()?;
            match field.as_str() {
                "name" => name = Some(input.parse()?),
                "mode" => {
                    let written: Ident = input.parse()?;
                    let Some(chosen) = ShaderMode::from_name(&written.to_string()) else {
                        return Err(syn::Error::new(
                            written.span(),
                            format!(
                                "`{written}` is no shading mode; write one of {}",
                                ShaderMode::ALL
                                    .iter()
                                    .map(|mode| mode.name())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        ));
                    };
                    mode = Some((chosen, written.span()));
                }
                "params" => params = Some(input.parse()?),
                "reach" => {
                    let written: syn::LitFloat = input.parse()?;
                    reach = written.base10_parse()?;
                    if reach < 0.0 {
                        return Err(syn::Error::new(
                            written.span(),
                            "an effect reads outside its box or it does not; a reach below zero \
                             would shrink the region it may read",
                        ));
                    }
                }
                "reads" => reads = parse_reads(input)?,
                "source" => source = Some(Source::Inline(input.parse()?)),
                "path" => source = Some(Source::Path(input.parse()?)),
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "`{other}` is no field of a shader declaration; write name, mode, \
                             params, reads, reach, and one of source or path"
                        ),
                    ));
                }
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }

        let span = Span::call_site();
        let name = name.ok_or_else(|| syn::Error::new(span, "a shader declares a `name`"))?;
        let (mode, mode_span) =
            mode.ok_or_else(|| syn::Error::new(span, "a shader declares a `mode`"))?;
        let source = source.ok_or_else(|| {
            syn::Error::new(span, "a shader declares its text, as either `source` or `path`")
        })?;
        Ok(Self {
            name,
            mode,
            mode_span,
            params,
            reads,
            reach,
            source,
        })
    }
}

/// Parses `[Time, Pointer]`.
fn parse_reads(input: ParseStream<'_>) -> syn::Result<ShaderReads> {
    let content;
    syn::bracketed!(content in input);
    let mut reads = ShaderReads::NOTHING;
    while !content.is_empty() {
        let written: Ident = content.parse()?;
        reads = reads.with(&written.to_string()).ok_or_else(|| {
            syn::Error::new(
                written.span(),
                format!(
                    "`{written}` is nothing an effect can declare it reads; write one of {}",
                    ShaderReads::NAMES.join(", ")
                ),
            )
        })?;
        if content.peek(Token![,]) {
            content.parse::<Token![,]>()?;
        }
    }
    Ok(reads)
}

/// Expands one invocation.
pub(crate) fn expand(input: TokenStream) -> Result<TokenStream, syn::Error> {
    let declared: Declaration = syn::parse2(input)?;
    let (snippet, tracked) = read_source(&declared.source)?;
    let unit = zgui_wgsl::effect(declared.mode, &snippet);

    // Asked before the unit is parsed, because the epilogue calls the function and a missing one
    // is reported by the shader front end as an unknown identifier deep in text the application
    // never wrote. This says the same thing where the application can act on it.
    if !mentions_entry(&snippet, declared.mode) {
        return Err(syn::Error::new(
            declared.mode_span,
            format!(
                "a `{}` effect writes `fn {}(in: ShaderInput, params: Params)`, and this shader \
                 declares no such function",
                declared.mode.name(),
                zgui_wgsl::entry(declared.mode)
            ),
        ));
    }
    let module = naga::front::wgsl::parse_str(&unit).map_err(|error| {
        syn::Error::new(
            source_span(&declared.source),
            format!(
                "this shader does not compile:\n{}",
                error.emit_to_string(&unit)
            ),
        )
    })?;
    // Parsing accepts text a device would refuse — a type that does not match, a value that is
    // never produced — so the same validation the driver would do is done here, where the error
    // lands on the line that caused it rather than at the first frame that draws.
    validate(&module).map_err(|error| {
        syn::Error::new(
            source_span(&declared.source),
            format!("this shader is not valid:\n{error}"),
        )
    })?;
    if !declares_entry(&module, declared.mode) {
        return Err(syn::Error::new(
            declared.mode_span,
            format!(
                "a `{}` effect writes `fn {}(in: ShaderInput, params: Params)`, and this shader \
                 declares no such function",
                declared.mode.name(),
                zgui_wgsl::entry(declared.mode)
            ),
        ));
    }
    let representation = bincode::serialize(&module).map_err(|error| {
        syn::Error::new(
            declared.name.span(),
            format!("this shader compiled but would not encode: {error}"),
        )
    })?;

    let name = &declared.name;
    let mode = mode_path(declared.mode);
    let reads_time = declared.reads.time;
    let reads_pointer = declared.reads.pointer;
    let reach = declared.reach;
    let params = match &declared.params {
        Some(spelled) => quote! { <#spelled as ::zgui::shader::ShaderParams>::LAYOUT },
        None => quote! { ::zgui::shader::ParamsLayout::EMPTY },
    };
    let bytes = syn::LitByteStr::new(&representation, Span::call_site());
    // Emitted so that rustc watches the file: an effect read from disk has to rebuild when the
    // disk changes, and only the compiler can be told that.
    let watch = tracked.map(|path| quote! { const _: &str = include_str!(#path); });

    Ok(quote! {
        {
            #watch
            ::zgui::shader::ShaderEffect::declared(
                ::zgui::shader::EffectProgram {
                    mode: #mode,
                    label: #name,
                    representation: #bytes,
                    source: #unit,
                    params: #params,
                },
                ::zgui::shader::ShaderReads {
                    time: #reads_time,
                    pointer: #reads_pointer,
                },
                #name,
                #reach,
            )
        }
    })
}

/// The application's own text, and the path rustc should watch for it.
fn read_source(source: &Source) -> Result<(String, Option<String>), syn::Error> {
    match source {
        Source::Inline(text) => Ok((text.value(), None)),
        Source::Path(relative) => {
            let root = std::env::var("CARGO_MANIFEST_DIR").map_err(|_| {
                syn::Error::new(
                    relative.span(),
                    "a shader read from a path needs cargo to say where the manifest is",
                )
            })?;
            let full = std::path::Path::new(&root).join(relative.value());
            let text = std::fs::read_to_string(&full).map_err(|error| {
                syn::Error::new(
                    relative.span(),
                    format!("{}: {error}", full.display()),
                )
            })?;
            Ok((text, Some(full.to_string_lossy().into_owned())))
        }
    }
}

/// Where an error in the application's own text is reported.
fn source_span(source: &Source) -> Span {
    match source {
        Source::Inline(text) => text.span(),
        Source::Path(path) => path.span(),
    }
}

/// Checks the module the way a device would.
fn validate(module: &naga::Module) -> Result<(), String> {
    // Everything an effect may use, so the check refuses what is wrong rather than what is merely
    // unavailable on one device. A capability a device genuinely lacks is the device's own error.
    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    validator
        .validate(module)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Whether the application's text appears to define the function the mode calls.
///
/// A textual answer, and deliberately generous: it decides only which of two messages an
/// application gets, and [`declares_entry`] is the one that decides whether the effect is accepted.
fn mentions_entry(snippet: &str, mode: ShaderMode) -> bool {
    let wanted = format!("fn {}", zgui_wgsl::entry(mode));
    snippet.match_indices(&wanted).any(|(at, matched)| {
        let after = snippet[at + matched.len()..].trim_start();
        after.starts_with('(')
    })
}

/// Whether the compiled module declares the function the mode calls.
fn declares_entry(module: &naga::Module, mode: ShaderMode) -> bool {
    module
        .functions
        .iter()
        .any(|(_, function)| function.name.as_deref() == Some(zgui_wgsl::entry(mode)))
}

/// The path naming one mode in the expansion.
fn mode_path(mode: ShaderMode) -> TokenStream {
    match mode {
        ShaderMode::Paint => quote! { ::zgui::shader::ShaderMode::Paint },
        ShaderMode::Coverage => quote! { ::zgui::shader::ShaderMode::Coverage },
        ShaderMode::Filter => quote! { ::zgui::shader::ShaderMode::Filter },
    }
}

#[cfg(test)]
mod tests {
    use super::expand;
    use quote::quote;

    /// What one invocation's expansion said, whether it succeeded or not.
    fn refusal(input: proc_macro2::TokenStream) -> String {
        expand(input)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| "the declaration was accepted".to_owned())
    }

    #[test]
    fn wgsl_that_does_not_parse_is_refused_with_the_front_end_s_own_message() {
        let refused = refusal(quote! {
            name: "broken",
            mode: Paint,
            source: "fn shade(in: ShaderInput, params: Params) -> vec4<f32> { return vec4<f32>(1.0) }",
        });
        assert!(refused.contains("does not compile"), "{refused}");
        // Naga's own message, rather than a summary of it: the application wrote the text and the
        // front end is the thing that read it.
        assert!(refused.contains("error"), "{refused}");
    }

    /// A shader whose types do not agree is refused with the expression that caused it.
    #[test]
    fn wgsl_whose_types_do_not_agree_is_refused() {
        let refused = refusal(quote! {
            name: "mistyped",
            mode: Paint,
            source: "fn shade(in: ShaderInput, params: Params) -> vec4<f32> { return 1.0; }",
        });
        assert!(refused.contains("vec4<f32>"), "{refused}");
        assert!(
            refused.contains("does not compile") || refused.contains("not valid"),
            "{refused}"
        );
    }

    #[test]
    fn a_shader_that_compiles_is_accepted() {
        let accepted = expand(quote! {
            name: "flat",
            mode: Paint,
            source: "fn shade(in: ShaderInput, params: Params) -> vec4<f32> { return vec4<f32>(1.0); }",
        });
        assert!(
            accepted.is_ok(),
            "{}",
            accepted.err().map(|error| error.to_string()).unwrap_or_default()
        );
    }

    /// Every mode's assembled unit compiles, which is the check that the epilogues and the prelude
    /// agree about what they declare and what they read.
    #[test]
    fn every_mode_assembles_a_unit_that_compiles() {
        for (mode, body) in [
            ("Paint", "fn shade(in: ShaderInput, params: Params) -> vec4<f32> { return vec4<f32>(1.0); }"),
            ("Coverage", "fn coverage(in: ShaderInput, params: Params) -> f32 { return 1.0; }"),
            (
                "Filter",
                "fn apply(in: ShaderInput, params: Params, beneath: texture_2d<f32>, \
                 beneath_sampler: sampler, region: FilterSource) -> vec4<f32> { \
                 return source_at(beneath, beneath_sampler, region, in.local); }",
            ),
        ] {
            let mode = syn::Ident::new(mode, proc_macro2::Span::call_site());
            let accepted = expand(quote! {
                name: "every-mode",
                mode: #mode,
                source: #body,
            });
            assert!(
                accepted.is_ok(),
                "{}",
                accepted.err().map(|e| e.to_string()).unwrap_or_default()
            );
        }
    }

    #[test]
    fn a_declaration_missing_its_text_names_what_it_is_missing() {
        let refused = refusal(quote! {
            name: "textless",
            mode: Paint,
        });
        assert!(refused.contains("source"), "{refused}");
    }

    #[test]
    fn a_field_written_twice_is_refused_rather_than_taking_the_last() {
        let refused = refusal(quote! {
            name: "first",
            name: "second",
            mode: Paint,
            source: "",
        });
        assert!(refused.contains("twice"), "{refused}");
    }
}
