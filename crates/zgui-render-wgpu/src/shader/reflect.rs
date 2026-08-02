//! Comparing what the shader says a structure is against what Rust says it is.

use crate::shader::layout::{Layout, parse};

/// One member of a Rust structure, as the comparison needs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Member {
    /// The field's name, which must be the shader's name for it too.
    pub name: &'static str,
    /// Its byte offset.
    pub offset: usize,
    /// How many bytes it occupies.
    pub size: usize,
}

/// Declares the Rust side of a structure's layout, read from the compiler rather than written out.
///
/// The offsets come from `offset_of!`, so this table cannot drift from the Rust structure; what it
/// is compared against is the shader's own text, which is the side that can.
macro_rules! reflected {
    ($type:ty, $wgsl:literal, [$($field:tt),+ $(,)?]) => {
        $crate::shader::reflect::Reflected {
            name: $wgsl,
            size: ::core::mem::size_of::<$type>(),
            members: vec![$(
                $crate::shader::reflect::Member {
                    name: stringify!($field),
                    offset: ::core::mem::offset_of!($type, $field),
                    size: ::core::mem::size_of_val(
                        &<$type as ::bytemuck::Zeroable>::zeroed().$field,
                    ),
                },
            )+],
        }
    };
}

pub(crate) use reflected;

/// One structure to compare: what the shader calls it, how big Rust says it is, and its members.
pub struct Reflected {
    /// The name of the structure in the shader.
    pub name: &'static str,
    /// The size of the Rust structure.
    pub size: usize,
    /// Its members.
    pub members: Vec<Member>,
}

/// Checks every structure in `structures` against its declaration in `source`.
///
/// Returns the first disagreement, phrased so that it names which side is which. A structure the
/// shader does not declare is itself a failure: it means the comparison silently covered nothing.
pub fn check(source: &str, structures: &[Reflected]) -> Result<(), String> {
    let layouts = parse(source);
    for structure in structures {
        let Some(layout) = layouts.get(structure.name) else {
            return Err(format!(
                "the shader declares no `{}`, so nothing was compared against it",
                structure.name
            ));
        };
        compare(structure, layout)?;
    }
    Ok(())
}

/// Compares one structure against one declaration.
fn compare(structure: &Reflected, layout: &Layout) -> Result<(), String> {
    let members = merged(layout).map_err(|error| format!("`{}`: {error}", structure.name))?;
    if layout.size != structure.size {
        return Err(format!(
            "`{}` is {} bytes in the shader and {} in Rust",
            structure.name, layout.size, structure.size
        ));
    }
    if members.len() != structure.members.len() {
        return Err(format!(
            "`{}` has {} members in the shader and {} in Rust",
            structure.name,
            members.len(),
            structure.members.len()
        ));
    }
    for (shader, rust) in members.iter().zip(&structure.members) {
        if shader.name != rust.name {
            return Err(format!(
                "`{}` member {} is `{}` in the shader and `{}` in Rust",
                structure.name, rust.offset, shader.name, rust.name
            ));
        }
        if shader.offset != rust.offset || shader.size != rust.size {
            return Err(format!(
                "`{}.{}` is {} bytes at offset {} in the shader and {} bytes at offset {} in Rust",
                structure.name, rust.name, shader.size, shader.offset, rust.size, rust.offset
            ));
        }
    }
    Ok(())
}

/// One entry per top-level member, with its nested leaves merged back together.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Merged {
    /// The member's name.
    name: String,
    /// Its offset.
    offset: usize,
    /// How many bytes it spans.
    size: usize,
}

/// The declaration's leaves, grouped back into the members they were declared as.
///
/// A gap between two leaves is a failure rather than a merge: contiguity is exactly the statement
/// that the structure has no padding, and a structure with padding cannot be copied as bytes
/// without reading memory nobody wrote.
fn merged(layout: &Layout) -> Result<Vec<Merged>, String> {
    let mut members: Vec<Merged> = Vec::new();
    for field in &layout.fields {
        let (head, _) = field.name.split_once('.').unwrap_or((&field.name, ""));
        match members.last_mut() {
            Some(last) if last.name == head => {
                if last.offset + last.size != field.offset {
                    return Err(format!("`{}` has padding before `{}`", head, field.name));
                }
                last.size += field.size;
            }
            _ => {
                if let Some(last) = members.last()
                    && last.offset + last.size != field.offset
                {
                    return Err(format!("has padding before `{}`", field.name));
                }
                members.push(Merged {
                    name: head.to_owned(),
                    offset: field.offset,
                    size: field.size,
                });
            }
        }
    }
    Ok(members)
}

#[cfg(test)]
mod tests {
    use super::{Member, Reflected, check};

    const QUAD: &str = "
        struct Bounds { x: f32, y: f32, w: f32, h: f32 }
        struct Thing { order: u32, bounds: Bounds, clip: u32 }
    ";

    fn thing(members: Vec<Member>, size: usize) -> Reflected {
        Reflected {
            name: "Thing",
            size,
            members,
        }
    }

    #[test]
    fn an_agreeing_pair_passes() {
        let members = vec![
            Member {
                name: "order",
                offset: 0,
                size: 4,
            },
            Member {
                name: "bounds",
                offset: 4,
                size: 16,
            },
            Member {
                name: "clip",
                offset: 20,
                size: 4,
            },
        ];
        assert!(check(QUAD, &[thing(members, 24)]).is_ok());
    }

    #[test]
    fn a_field_at_the_wrong_offset_is_named() {
        let members = vec![
            Member {
                name: "order",
                offset: 0,
                size: 4,
            },
            Member {
                name: "bounds",
                offset: 8,
                size: 16,
            },
            Member {
                name: "clip",
                offset: 24,
                size: 4,
            },
        ];
        let error = check(QUAD, &[thing(members, 28)]).unwrap_err();
        assert!(error.contains("Thing"), "{error}");
    }

    #[test]
    fn a_structure_the_shader_does_not_declare_fails_rather_than_passing_vacuously() {
        let error = check("struct Other { a: f32 }", &[thing(Vec::new(), 0)]).unwrap_err();
        assert!(error.contains("no `Thing`"), "{error}");
    }

    #[test]
    fn padding_in_the_shader_declaration_is_a_failure() {
        let source = "struct Thing { a: u32, b: vec4<f32> }";
        let members = vec![
            Member {
                name: "a",
                offset: 0,
                size: 4,
            },
            Member {
                name: "b",
                offset: 4,
                size: 16,
            },
        ];
        let error = check(source, &[thing(members, 20)]).unwrap_err();
        assert!(error.contains("padding"), "{error}");
    }
}
