//! The `Params` structure a compiled shader declares.

use proc_macro2::TokenStream;
use quote::quote;

/// One member of `Params` as the shader lays it out.
pub(crate) struct Member {
    name: String,
    offset: u32,
    /// The bytes from the member's offset to the next member's, or to the end of the structure.
    size: u32,
}

/// The layout of `Params` in `module`, as its size and members, or `None` when the module declares
/// no such structure.
pub(crate) fn params(module: &naga::Module) -> Option<(u32, Vec<Member>)> {
    let (members, span) = module.types.iter().find_map(|(_, ty)| match &ty.inner {
        naga::TypeInner::Struct { members, span } if ty.name.as_deref() == Some("Params") => {
            Some((members, *span))
        }
        _ => None,
    })?;
    let ends = members
        .iter()
        .skip(1)
        .map(|member| member.offset)
        .chain([span]);
    let laid = members
        .iter()
        .zip(ends)
        .map(|(member, end)| Member {
            name: member.name.clone().unwrap_or_default(),
            offset: member.offset,
            size: end - member.offset,
        })
        .collect();
    Some((span, laid))
}

/// Returns the expression of a `ParamsLayout` holding `size` and `members`.
pub(crate) fn tokens(size: u32, members: &[Member]) -> TokenStream {
    let size = size as usize;
    let fields = members.iter().map(|member| {
        let name = &member.name;
        let offset = member.offset as usize;
        let size = member.size as usize;
        quote! {
            ::zgui::shader::ParamsField { name: #name, offset: #offset, size: #size }
        }
    });
    quote! {
        ::zgui::shader::ParamsLayout { size: #size, fields: &[#(#fields),*] }
    }
}
