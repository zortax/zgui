//! The names a node may not be given.
//!
//! `for` and `if` begin control flow, and the five words that keep them company are held back from
//! the element vocabulary so that the shapes a reader expects to work either work or say why. A
//! view that names one of them is told so where it wrote it, instead of being told that a function
//! is missing from a module it never mentioned.

use syn::ext::IdentExt;
use syn::parse::ParseStream;

/// The words a node may not be named after, and what each one is told.
const RESERVED: [(&str, &str); 5] = [
    (
        "else",
        "`else` is written after the block of an `if`, and belongs to it",
    ),
    (
        "in",
        "`in` is written after the row of a `for`, and belongs to it",
    ),
    ("while", WORDLESS),
    ("loop", WORDLESS),
    ("match", WORDLESS),
];

/// What a word with no meaning in a view is told, with its own name filled in.
const WORDLESS: &str = "`{}` is reserved and has no meaning in a view\n\n\
                        note: `for` and `if` are the control flow a view has; anything else is a \
                        component";

/// Fails when the name about to be read is one of the reserved words.
pub(crate) fn check(input: ParseStream<'_>) -> syn::Result<()> {
    let fork = input.fork();
    let Ok(ident) = syn::Ident::parse_any(&fork) else {
        return Ok(());
    };
    let name = ident.to_string();
    let Some((_, message)) = RESERVED.iter().find(|(word, _)| *word == name) else {
        return Ok(());
    };
    Err(syn::Error::new(ident.span(), message.replace("{}", &name)))
}
