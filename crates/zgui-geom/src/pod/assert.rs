//! The compile-time layout table.
//!
//! Every entry states a type's size, its alignment, and the offset and size of each of its fields.
//! The checks that follow from that are stronger than they look: the offsets are compared against
//! what the compiler actually chose, and the field sizes are required to add up to the struct size
//! with no gaps, which is exactly the statement that the type contains no padding bytes. All of it
//! is evaluated while the crate is compiled, so a layout mistake cannot reach a buffer upload.
//!
//! The table is the reason [`bytemuck::Pod`] can be promised for these types, and the reason a
//! shader's declaration of the same struct can be trusted to line up with this one.

/// Asserts a type's size, alignment, field offsets and freedom from padding.
///
/// The fields must be listed in declaration order, each with the byte offset it is expected at and
/// its size in bytes. A zero-sized field, such as a space marker, is written with a size of zero.
macro_rules! assert_layout {
    (
        $type:ty,
        size = $size:expr,
        align = $align:expr,
        fields = [$($field:tt @ $offset:expr, $width:expr);+ $(;)?]
        $(,)?
    ) => {
        const _: () = {
            assert!(
                ::core::mem::size_of::<$type>() == $size,
                concat!(stringify!($type), " is not the size the layout table claims"),
            );
            assert!(
                ::core::mem::align_of::<$type>() == $align,
                concat!(stringify!($type), " is not aligned the way the layout table claims"),
            );
            $(
                assert!(
                    ::core::mem::offset_of!($type, $field) == $offset,
                    concat!(
                        stringify!($type),
                        ".",
                        stringify!($field),
                        " is not at the offset the layout table claims",
                    ),
                );
            )+
            // Walking the fields end to end proves there is no padding: every field has to begin
            // exactly where the previous one ended, and the last has to end at the struct's size.
            let mut cursor: usize = 0;
            $(
                assert!(
                    $offset == cursor,
                    concat!(
                        stringify!($type),
                        " has padding before ",
                        stringify!($field),
                    ),
                );
                cursor += $width;
            )+
            assert!(
                cursor == $size,
                concat!(stringify!($type), " has trailing padding"),
            );
        };
    };
}

use crate::corners::{Corners, Vec2};
use crate::edges::Edges;
use crate::point::Point;
use crate::rect::Rect;
use crate::size::Size;
use crate::space::{Css, Device, Layout};
use crate::transform::{Affine2, Decomposed, Matrix4};
use crate::unit::{Au, CssPx, DevicePx, Scale};

assert_layout!(CssPx, size = 4, align = 4, fields = [0 @ 0, 4]);
assert_layout!(DevicePx, size = 4, align = 4, fields = [0 @ 0, 4]);
assert_layout!(Au, size = 4, align = 4, fields = [0 @ 0, 4]);

assert_layout!(
    Scale<Css, Device>,
    size = 4,
    align = 4,
    fields = [factor @ 0, 4; spaces @ 4, 0],
);

assert_layout!(
    Point<CssPx, Css>,
    size = 8,
    align = 4,
    fields = [x @ 0, 4; y @ 4, 4; space @ 8, 0],
);
assert_layout!(
    Point<DevicePx, Device>,
    size = 8,
    align = 4,
    fields = [x @ 0, 4; y @ 4, 4; space @ 8, 0],
);
assert_layout!(
    Point<Au, Layout>,
    size = 8,
    align = 4,
    fields = [x @ 0, 4; y @ 4, 4; space @ 8, 0],
);
assert_layout!(
    Point<i32, Device>,
    size = 8,
    align = 4,
    fields = [x @ 0, 4; y @ 4, 4; space @ 8, 0],
);

assert_layout!(
    Size<CssPx, Css>,
    size = 8,
    align = 4,
    fields = [width @ 0, 4; height @ 4, 4; space @ 8, 0],
);
assert_layout!(
    Size<DevicePx, Device>,
    size = 8,
    align = 4,
    fields = [width @ 0, 4; height @ 4, 4; space @ 8, 0],
);

assert_layout!(
    Rect<CssPx, Css>,
    size = 16,
    align = 4,
    fields = [origin @ 0, 8; size @ 8, 8],
);
assert_layout!(
    Rect<DevicePx, Device>,
    size = 16,
    align = 4,
    fields = [origin @ 0, 8; size @ 8, 8],
);
assert_layout!(
    Rect<i32, Device>,
    size = 16,
    align = 4,
    fields = [origin @ 0, 8; size @ 8, 8],
);

assert_layout!(
    Vec2<DevicePx>,
    size = 8,
    align = 4,
    fields = [x @ 0, 4; y @ 4, 4],
);

assert_layout!(
    Corners<CssPx>,
    size = 16,
    align = 4,
    fields = [top_left @ 0, 4; top_right @ 4, 4; bottom_right @ 8, 4; bottom_left @ 12, 4],
);
assert_layout!(
    Corners<Vec2<DevicePx>>,
    size = 32,
    align = 4,
    fields = [top_left @ 0, 8; top_right @ 8, 8; bottom_right @ 16, 8; bottom_left @ 24, 8],
);

assert_layout!(
    Edges<DevicePx>,
    size = 16,
    align = 4,
    fields = [top @ 0, 4; right @ 4, 4; bottom @ 8, 4; left @ 12, 4],
);

assert_layout!(
    Affine2,
    size = 24,
    align = 4,
    fields = [a @ 0, 4; b @ 4, 4; c @ 8, 4; d @ 12, 4; tx @ 16, 4; ty @ 20, 4],
);

assert_layout!(Matrix4, size = 64, align = 4, fields = [columns @ 0, 64]);

assert_layout!(
    Decomposed,
    size = 68,
    align = 4,
    fields = [
        translation @ 0, 12;
        scale @ 12, 12;
        skew @ 24, 12;
        perspective @ 36, 16;
        rotation @ 52, 16;
    ],
);

#[cfg(test)]
mod tests {
    use core::mem::{align_of, size_of};

    use crate::corners::{Corners, Vec2};
    use crate::point::Point;
    use crate::rect::Rect;
    use crate::space::{Css, Device};
    use crate::unit::{CssPx, DevicePx};

    #[test]
    fn the_table_matches_what_the_compiler_chose() {
        assert_eq!(size_of::<Point<CssPx, Css>>(), 8);
        assert_eq!(align_of::<Point<CssPx, Css>>(), 4);
        assert_eq!(size_of::<Rect<DevicePx, Device>>(), 16);
        assert_eq!(size_of::<Corners<Vec2<DevicePx>>>(), 32);
    }

    #[test]
    fn a_space_marker_costs_nothing() {
        assert_eq!(
            size_of::<Point<CssPx, Css>>(),
            size_of::<[CssPx; 2]>(),
            "the space marker must not add a byte",
        );
    }
}
