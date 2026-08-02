//! Plain-old-data guarantees for the types that cross to the GPU.
//!
//! A vertex or storage buffer is a byte array. Filling one from a Rust value means promising three
//! things about that value: that its layout is the one C would give it, that every bit pattern is
//! a valid value of the type, and that it contains no padding bytes. The third is the one that
//! bites — reading uninitialised padding is undefined behaviour, and a struct whose Rust layout
//! only *happens* to match the shader's declaration will keep working until someone reorders a
//! field.
//!
//! Every geometry type in this crate is `#[repr(C)]`, implements [`bytemuck::Pod`], and appears in
//! the layout table in [`mod@assert`], where its size, alignment and field offsets are checked when the
//! crate is compiled. Getting any of it wrong is a build failure, not a rendering artefact.
//!
//! ```
//! use zgui_geom::{Css, CssPx, Point, Rect, Size};
//!
//! let rect: Rect<CssPx, Css> = Rect::new(
//!     Point::new(CssPx(1.0), CssPx(2.0)),
//!     Size::new(CssPx(3.0), CssPx(4.0)),
//! );
//! assert_eq!(bytemuck::bytes_of(&rect).len(), 16);
//! ```

// The plain-old-data promises below are the only unsafe code in this crate. Each one is a claim
// about layout, and each claim is checked at compile time by the table in `assert`.
#![allow(unsafe_code)]

pub mod assert;

use bytemuck::{Pod, Zeroable};

use crate::corners::{Corners, Vec2};
use crate::edges::Edges;
use crate::point::Point;
use crate::rect::Rect;
use crate::size::Size;
use crate::space::Space;
use crate::transform::{Affine2, Decomposed, Matrix4};
use crate::unit::{Au, CssPx, DevicePx, Scale};

// SAFETY: a `#[repr(transparent)]` wrapper has exactly the layout, size and validity of the type
// it wraps, and `f32` and `i32` are themselves plain old data with no invalid bit patterns.
unsafe impl Zeroable for CssPx {}
// SAFETY: as above.
unsafe impl Pod for CssPx {}
// SAFETY: as above.
unsafe impl Zeroable for DevicePx {}
// SAFETY: as above.
unsafe impl Pod for DevicePx {}
// SAFETY: as above.
unsafe impl Zeroable for Au {}
// SAFETY: as above.
unsafe impl Pod for Au {}

// SAFETY: `Scale` is `#[repr(transparent)]` over an `f32`; the space markers appear only inside a
// `PhantomData`, which occupies no bytes, so the layout is exactly that of an `f32`.
unsafe impl<Src: Space, Dst: Space> Zeroable for Scale<Src, Dst> {}
// SAFETY: as above.
unsafe impl<Src: Space, Dst: Space> Pod for Scale<Src, Dst> {}

// SAFETY: `#[repr(C)]` with two fields of the same plain-old-data type, so they are adjacent and
// the struct's size is exactly twice the field size — there is nowhere for padding to appear. The
// space marker is a `PhantomData` and occupies no bytes.
unsafe impl<T: Zeroable, S: Space> Zeroable for Point<T, S> {}
// SAFETY: as above, and every bit pattern of the fields is valid because `T: Pod`.
unsafe impl<T: Pod, S: Space> Pod for Point<T, S> {}

// SAFETY: as for `Point`.
unsafe impl<T: Zeroable, S: Space> Zeroable for Size<T, S> {}
// SAFETY: as for `Point`.
unsafe impl<T: Pod, S: Space> Pod for Size<T, S> {}

// SAFETY: `#[repr(C)]` with a `Point<T, S>` followed by a `Size<T, S>`. Both have the same size
// and alignment and neither contains padding, so neither does the pair.
unsafe impl<T: Zeroable, S: Space> Zeroable for Rect<T, S> {}
// SAFETY: as above, and every bit pattern of the fields is valid because `T: Pod`.
unsafe impl<T: Pod, S: Space> Pod for Rect<T, S> {}

// SAFETY: `#[repr(C)]` with two fields of the same plain-old-data type.
unsafe impl<T: Zeroable> Zeroable for Vec2<T> {}
// SAFETY: as above, and every bit pattern of the fields is valid because `T: Pod`.
unsafe impl<T: Pod> Pod for Vec2<T> {}

// SAFETY: `#[repr(C)]` with four fields of the same plain-old-data type.
unsafe impl<T: Zeroable> Zeroable for Corners<T> {}
// SAFETY: as above, and every bit pattern of the fields is valid because `T: Pod`.
unsafe impl<T: Pod> Pod for Corners<T> {}

// SAFETY: `#[repr(C)]` with four fields of the same plain-old-data type.
unsafe impl<T: Zeroable> Zeroable for Edges<T> {}
// SAFETY: as above, and every bit pattern of the fields is valid because `T: Pod`.
unsafe impl<T: Pod> Pod for Edges<T> {}

// SAFETY: `#[repr(C)]` with six `f32` fields, which are equally sized and aligned, so the struct
// is exactly six floats with no padding and every bit pattern is a valid value.
unsafe impl Zeroable for Affine2 {}
// SAFETY: as above.
unsafe impl Pod for Affine2 {}

// SAFETY: `#[repr(C)]` around a single `[[f32; 4]; 4]`, which is sixteen contiguous floats.
unsafe impl Zeroable for Matrix4 {}
// SAFETY: as above.
unsafe impl Pod for Matrix4 {}

// SAFETY: `#[repr(C)]` with five `f32` arrays. Every field has alignment four and a size that is a
// multiple of four, so the fields are contiguous and the struct has no trailing padding.
unsafe impl Zeroable for Decomposed {}
// SAFETY: as above.
unsafe impl Pod for Decomposed {}

#[cfg(test)]
mod tests {
    use crate::corners::{Corners, Vec2};
    use crate::point::Point;
    use crate::rect::Rect;
    use crate::size::Size;
    use crate::space::{Css, Device};
    use crate::transform::Matrix4;
    use crate::unit::{CssPx, DevicePx};

    #[test]
    fn geometry_maps_onto_its_bytes_and_back() {
        let rect: Rect<CssPx, Css> = Rect::new(
            Point::new(CssPx(1.0), CssPx(2.0)),
            Size::new(CssPx(3.0), CssPx(4.0)),
        );
        let bytes = bytemuck::bytes_of(&rect);
        assert_eq!(bytes.len(), 16);
        assert_eq!(
            bytemuck::pod_read_unaligned::<Rect<CssPx, Css>>(bytes),
            rect
        );
    }

    #[test]
    fn the_bytes_are_the_fields_in_declaration_order() {
        let rect: Rect<CssPx, Css> = Rect::new(
            Point::new(CssPx(1.0), CssPx(2.0)),
            Size::new(CssPx(3.0), CssPx(4.0)),
        );
        let floats: &[f32] = bytemuck::cast_slice(bytemuck::bytes_of(&rect));
        assert_eq!(floats, &[1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn elliptical_radii_are_eight_floats_in_corner_order() {
        let radii: Corners<Vec2<DevicePx>> = Corners::new(
            Vec2::new(DevicePx(1.0), DevicePx(2.0)),
            Vec2::new(DevicePx(3.0), DevicePx(4.0)),
            Vec2::new(DevicePx(5.0), DevicePx(6.0)),
            Vec2::new(DevicePx(7.0), DevicePx(8.0)),
        );
        let floats: &[f32] = bytemuck::cast_slice(bytemuck::bytes_of(&radii));
        assert_eq!(floats, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
    }

    #[test]
    fn a_matrix_is_sixteen_floats_in_column_order() {
        let matrix = Matrix4::translation(7.0, 8.0, 9.0);
        let floats: &[f32] = bytemuck::cast_slice(bytemuck::bytes_of(&matrix));
        assert_eq!(&floats[12..], &[7.0, 8.0, 9.0, 1.0]);
    }

    #[test]
    fn a_zeroed_point_is_the_origin() {
        let point: Point<DevicePx, Device> = bytemuck::Zeroable::zeroed();
        assert_eq!(point, Point::ORIGIN);
    }
}
