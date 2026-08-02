//! Comparing the part of a style struct a question is about.

/// Whether two styles agree about every named field of one style struct.
///
/// The pointer test first is what makes this affordable to run over a hundred and fifty properties
/// on every restyle: the engine shares a struct between two styles whenever the cascade produced
/// nothing for it, so most structs are settled in one comparison. It is only a fast path, never an
/// answer on its own — a struct the cascade did rebuild has a fresh address whether or not any
/// value in it moved, which is exactly the case this whole classification exists to see through.
///
/// The fields are named rather than the struct compared whole, because a struct mixes properties
/// that cost a layout with properties that cost a repaint, and comparing it whole would report the
/// second as the first.
macro_rules! same {
    ($old:expr, $new:expr, $struct:ident, [$($field:ident),+ $(,)?]) => {{
        let old = $old.$struct();
        let new = $new.$struct();
        ::core::ptr::eq(old, new) || !($(old.$field != new.$field ||)+ false)
    }};
}

pub(super) use same;
