//! The compile-time layout table for every instance struct.
//!
//! Each entry states a struct's size, its alignment, and the offset and size of every field in
//! declaration order. Two things are checked, and the second is the one that matters: the offsets
//! are compared against what the compiler actually chose, and the field sizes are required to add
//! up to the struct size with no gaps — which is exactly the statement that the struct contains no
//! padding bytes, and therefore that copying it into a buffer never reads uninitialised memory.
//!
//! All of it is evaluated while the crate is compiled. Reordering a field, inserting one, or
//! changing a type is a build failure here rather than a rendering artefact somewhere else.

use zgui_geom::{Device, DevicePx, Point, Rect, Size};

/// Asserts a type's size, alignment, field offsets and freedom from padding.
///
/// Fields are listed in declaration order, each with the byte offset it is expected at and its size
/// in bytes.
macro_rules! assert_instance_layout {
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
                        stringify!($type), ".", stringify!($field),
                        " is not at the offset the layout table claims",
                    ),
                );
            )+
            // Walking the fields end to end proves there is no padding: each must begin exactly
            // where the previous one ended, and the last must end at the struct's size.
            let mut cursor: usize = 0;
            $(
                assert!(
                    $offset == cursor,
                    concat!(stringify!($type), " has padding before ", stringify!($field)),
                );
                cursor += $width;
            )+
            assert!(cursor == $size, concat!(stringify!($type), " has trailing padding"));
        };
    };
}

use crate::paint::PaintRef;
use crate::prim::decoration::Decoration;
use crate::prim::quad::Quad;
use crate::prim::shadow::Shadow;
use crate::prim::sprite::{ColorSprite, MonoSprite, SpriteTile, SubpixelSprite};

assert_instance_layout!(
    PaintRef,
    size = 8,
    align = 4,
    fields = [kind @ 0, 4; index @ 4, 4],
);

assert_instance_layout!(
    SpriteTile,
    size = 24,
    align = 4,
    fields = [texture @ 0, 4; tile @ 4, 4; bounds @ 8, 16],
);

assert_instance_layout!(
    Quad,
    size = 104,
    align = 4,
    fields = [
        order @ 0, 4;
        style @ 4, 4;
        bounds @ 8, 16;
        radii @ 24, 32;
        border @ 56, 16;
        fill @ 72, 8;
        stroke @ 80, 8;
        clip @ 88, 4;
        transform @ 92, 4;
        paint_origin @ 96, 8;
    ],
);

assert_instance_layout!(
    Shadow,
    size = 136,
    align = 4,
    fields = [
        order @ 0, 4;
        blur @ 4, 4;
        bounds @ 8, 16;
        radii @ 24, 32;
        element_bounds @ 56, 16;
        element_radii @ 72, 32;
        color @ 104, 16;
        clip @ 120, 4;
        transform @ 124, 4;
        inset @ 128, 4;
        reserved @ 132, 4;
    ],
);

assert_instance_layout!(
    Decoration,
    size = 56,
    align = 4,
    fields = [
        order @ 0, 4;
        style @ 4, 4;
        bounds @ 8, 16;
        color @ 24, 16;
        thickness @ 40, 4;
        clip @ 44, 4;
        transform @ 48, 4;
        reserved @ 52, 4;
    ],
);

assert_instance_layout!(
    MonoSprite,
    size = 72,
    align = 4,
    fields = [
        order @ 0, 4;
        reserved @ 4, 4;
        bounds @ 8, 16;
        color @ 24, 16;
        tile @ 40, 24;
        clip @ 64, 4;
        transform @ 68, 4;
    ],
);

assert_instance_layout!(
    SubpixelSprite,
    size = 72,
    align = 4,
    fields = [
        order @ 0, 4;
        reserved @ 4, 4;
        bounds @ 8, 16;
        color @ 24, 16;
        tile @ 40, 24;
        clip @ 64, 4;
        transform @ 68, 4;
    ],
);

assert_instance_layout!(
    ColorSprite,
    size = 92,
    align = 4,
    fields = [
        order @ 0, 4;
        flags @ 4, 4;
        bounds @ 8, 16;
        radii @ 24, 32;
        tile @ 56, 24;
        opacity @ 80, 4;
        clip @ 84, 4;
        transform @ 88, 4;
    ],
);

/// The rectangle a `[x, y, width, height]` instance field describes.
pub(crate) fn rect_of(bounds: [f32; 4]) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(bounds[0]), DevicePx(bounds[1])),
        Size::new(DevicePx(bounds[2]), DevicePx(bounds[3])),
    )
}

#[cfg(test)]
mod tests {
    use core::mem::offset_of;

    use crate::prim::{ColorSprite, Decoration, MonoSprite, Quad, Shadow, SubpixelSprite};

    /// The table above is compile-time; this is the runtime half, so a failure names the type.
    #[test]
    fn the_table_matches_what_the_compiler_chose() {
        assert_eq!(size_of::<Quad>(), 104);
        assert_eq!(size_of::<Shadow>(), 136);
        assert_eq!(size_of::<Decoration>(), 56);
        assert_eq!(size_of::<MonoSprite>(), 72);
        assert_eq!(size_of::<SubpixelSprite>(), 72);
        assert_eq!(size_of::<ColorSprite>(), 92);
    }

    /// A subpixel sprite differs from a monochrome one only in which pipeline draws it, so the two
    /// must stay byte-identical or one of them is silently a different instance format.
    #[test]
    fn the_two_coverage_sprites_share_a_layout() {
        assert_eq!(size_of::<MonoSprite>(), size_of::<SubpixelSprite>());
        assert_eq!(
            offset_of!(MonoSprite, tile),
            offset_of!(SubpixelSprite, tile)
        );
    }

    /// Every instance is copyable as bytes, which is only true because it has no padding.
    #[test]
    fn an_instance_round_trips_through_its_bytes() {
        let quad = Quad::default();
        let bytes = bytemuck::bytes_of(&quad);
        assert_eq!(bytes.len(), 104);
        assert_eq!(bytemuck::pod_read_unaligned::<Quad>(bytes), quad);
    }
}
