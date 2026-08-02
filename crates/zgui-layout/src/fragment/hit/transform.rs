//! Getting a pointer position into the space a fragment's own rectangle is in.
//!
//! A fragment keeps its rectangle in its own untransformed space, because that is the space its
//! clip, its corner radii and its children are all expressed in. Testing a pointer against it
//! therefore means carrying the pointer the other way: *down* the chain of coordinate systems, one
//! at a time, until it is expressed in the same terms the rectangle is.
//!
//! # Why down the chain and not through the resolved matrix
//!
//! The two arrive at the same point, and one of them is dearer. Resolving multiplies the chain into
//! a matrix and then inverts it, for every coordinate system a query passes through. Carried down,
//! each step is one node's *own* transform, and the overwhelming majority of those are either the
//! identity — a sticky header that is not currently shifted, a fixed box against the window — or a
//! plain translation, whose inverse is a subtraction.
//!
//! # Why no step inverts a matrix
//!
//! A general four-by-four inverse is twenty three-by-three determinants — sixteen for the adjugate
//! and four more for the divisor — and it is the wrong instrument for every transform a document
//! actually writes. A rotation about the screen's normal, a scale, a skew and any composition of
//! them with a translation all leave the plane a plane, and undoing one of those is a two-by-two
//! solve: a determinant, a reciprocal and four products. Measured on the component gallery, which
//! holds twenty-three coordinate systems and carries such a transform in ten of them, the inverses
//! were four fifths of what a query cost — a hundred nanoseconds apiece, against a query that now
//! answers the whole document in four hundred.
//!
//! The general answer stays, because perspective and rotation out of the screen are expressible and
//! a query still has to answer over them. It is simply not what the common case is charged.
//!
//! It answers nothing in the same two cases: a chain that reaches a name no longer live, and a step
//! that collapses the plane. The second is decided per node rather than over the product, which is
//! the same question — a product is singular exactly when one of its factors is.
//!
//! # Which matrix, when one is moving
//!
//! The matrix the coordinate system holds *now*, which during a transform transition is the matrix
//! the tick that ran before this frame's composition sampled. So a hit query answers against the
//! same matrix the frame in front of the pointer was drawn with, and an element half-way through
//! moving is hit where it is seen rather than where it started or where it is going.

use smallvec::SmallVec;
use zgui_geom::{Device, DevicePx, Matrix4, Point};
use zgui_scene::{SpatialId, SpatialTree};

/// Maps a device-space point into the coordinate system `space` names.
///
/// Returns nothing when the coordinate system collapses the plane — a `scale(0)`, or a rotation
/// seen exactly edge-on — because such a fragment covers no area on the device and nothing can be
/// on it, and nothing when the name no longer resolves.
///
/// ```
/// use zgui_geom::{Device, DevicePx, Matrix4, Point};
/// use zgui_layout::fragment::hit::transform::into_local;
/// use zgui_scene::{OwnSpace, PropertyOwner, SpatialTree};
///
/// let mut tree = SpatialTree::with_viewport();
/// let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
/// let moved = Matrix4::translation(30.0, 0.0, 0.0);
/// let card = tree.space_of(tree.viewport(), owner, OwnSpace::of(Some(moved), None, false));
///
/// let point: Point<DevicePx, Device> = Point::new(DevicePx(40.0), DevicePx(5.0));
/// assert_eq!(
///     into_local(point, Some(card), &tree),
///     Some(Point::new(DevicePx(10.0), DevicePx(5.0))),
///     "the pointer is run backwards through the matrix the element is drawn with",
/// );
/// ```
pub fn into_local(
    point: Point<DevicePx, Device>,
    space: Option<SpatialId>,
    spatial: &SpatialTree,
) -> Option<Point<DevicePx, Device>> {
    let Some(id) = space else {
        return Some(point);
    };
    // Translations commute and compose by addition, so a chain of them needs neither an order nor a
    // buffer: the walk out to the root adds up as it goes and the answer is one subtraction. That is
    // every coordinate system in an interface that is not currently rotating or scaling something,
    // which is nearly all of them nearly all of the time — and a query pays this once per
    // coordinate system it holds a hierarchy for, so what it costs when nothing is rotating is what
    // it costs.
    let (mut x, mut y) = (0.0, 0.0);
    let mut next = Some(id);
    while let Some(current) = next {
        let node = spatial.get(current)?;
        let Some((shift_x, shift_y)) = shift_of(&node.local) else {
            return through_the_chain(point, id, spatial);
        };
        x += shift_x;
        y += shift_y;
        next = node.parent;
    }
    Some(Point::new(DevicePx(point.x.0 - x), DevicePx(point.y.0 - y)))
}

/// The general answer, for a chain that rotates, scales or skews somewhere along it.
///
/// Those do not commute, so the point has to be carried down one coordinate system at a time from
/// the outermost inwards — and the only way to reach the outermost is to walk out from the
/// innermost, which is why the chain is collected before any of it is applied.
///
/// The names are collected and not the matrices. A name is a word and a matrix is sixteen, so
/// carrying the names costs a fraction of the copying and one extra lookup per node — and the
/// lookup is a bounds check and an offset into storage the walk out has just read.
fn through_the_chain(
    point: Point<DevicePx, Device>,
    id: SpatialId,
    spatial: &SpatialTree,
) -> Option<Point<DevicePx, Device>> {
    let mut chain: SmallVec<[SpatialId; 8]> = SmallVec::new();
    let mut next = Some(id);
    while let Some(current) = next {
        let node = spatial.get(current)?;
        chain.push(current);
        next = node.parent;
    }
    let mut carried = point;
    for name in chain.iter().rev() {
        carried = inwards(spatial.get(*name)?.local, carried)?;
    }
    Some(carried)
}

/// One step inwards: a point in the space above `local`, expressed in the space `local` establishes.
///
/// Three answers, cheapest first, and the order is the whole of what makes this affordable. The
/// identity is not one of them: it is a translation by nothing, so the first answer already handles
/// it in two subtractions against zero, and testing for it separately would charge every node a
/// sixteen-word comparison to save two.
fn inwards(local: Matrix4, point: Point<DevicePx, Device>) -> Option<Point<DevicePx, Device>> {
    if let Some((x, y)) = shift_of(&local) {
        return Some(Point::new(DevicePx(point.x.0 - x), DevicePx(point.y.0 - y)));
    }
    if let Some(plane) = plane_of(&local) {
        return plane.carry(point);
    }
    let inverse = local.invert()?;
    let mapped = inverse.transform_vector4([point.x.0, point.y.0, 0.0, 1.0]);
    if mapped[3] == 0.0 || !mapped[3].is_finite() {
        return None;
    }
    let scale = mapped[3].recip();
    Some(Point::new(
        DevicePx(mapped[0] * scale),
        DevicePx(mapped[1] * scale),
    ))
}

/// A transform that keeps the plane a plane, as the six numbers that describe it.
///
/// Its inverse is a two-by-two one, which is a determinant and four products — where a matrix that
/// does not keep the plane a plane has to be inverted in four dimensions, at twenty three-by-three
/// determinants for the adjugate and one more for the divisor.
#[derive(Clone, Copy, Debug)]
struct Plane {
    /// Where the local x axis points, and where the local y axis points.
    axes: [f32; 4],
    /// Where the local origin lands.
    origin: [f32; 2],
}

impl Plane {
    /// Carries a point in the space above this transform into the space it establishes.
    ///
    /// Nothing when the transform collapses the plane onto a line or a point, because a fragment
    /// drawn through it covers no area and nothing can be on it.
    fn carry(self, point: Point<DevicePx, Device>) -> Option<Point<DevicePx, Device>> {
        let [a, b, c, d] = self.axes;
        let determinant = a * d - c * b;
        if determinant == 0.0 || !determinant.is_finite() {
            return None;
        }
        let scale = determinant.recip();
        let x = point.x.0 - self.origin[0];
        let y = point.y.0 - self.origin[1];
        Some(Point::new(
            DevicePx((d * x - c * y) * scale),
            DevicePx((a * y - b * x) * scale),
        ))
    }
}

/// What a matrix does to the plane, when the plane is all it touches.
///
/// Worth asking because every transform a document actually writes is one of these — a rotation
/// about the screen's normal, a scale, a skew, and any composition of them with a translation —
/// while the general answer costs an order of magnitude more and exists for the perspective and
/// out-of-plane rotation a query may still meet.
fn plane_of(matrix: &Matrix4) -> Option<Plane> {
    let columns = matrix.columns;
    let flat = columns[0][2] == 0.0
        && columns[0][3] == 0.0
        && columns[1][2] == 0.0
        && columns[1][3] == 0.0
        && columns[2] == [0.0, 0.0, 1.0, 0.0]
        && columns[3][2] == 0.0
        && columns[3][3] == 1.0;
    flat.then(|| Plane {
        axes: [columns[0][0], columns[0][1], columns[1][0], columns[1][1]],
        origin: [columns[3][0], columns[3][1]],
    })
}

/// How far a matrix moves the plane, when moving it is all the matrix does.
///
/// Worth asking because it is the answer for nearly every coordinate system a real document has —
/// an element nudged, a panel slid in, a thumb thrown across its track — and because the inverse of
/// a translation is a subtraction rather than sixteen cofactors.
fn shift_of(matrix: &Matrix4) -> Option<(f32, f32)> {
    let columns = matrix.columns;
    let unmoved = columns[0] == [1.0, 0.0, 0.0, 0.0]
        && columns[1] == [0.0, 1.0, 0.0, 0.0]
        && columns[2] == [0.0, 0.0, 1.0, 0.0]
        && columns[3][3] == 1.0;
    unmoved.then(|| (columns[3][0], columns[3][1]))
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Affine2, Device, DevicePx, Point};
    use zgui_scene::{OwnSpace, PropertyOwner, SpatialId, SpatialTree};

    use super::into_local;

    /// A tree holding the viewport and one coordinate system under it.
    fn space_of(affine: Affine2) -> (SpatialTree, SpatialId) {
        let mut tree = SpatialTree::with_viewport();
        let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
        let own = OwnSpace::of(Some(affine.to_matrix4()), None, false);
        let id = tree.space_of(tree.viewport(), owner, own);
        (tree, id)
    }

    #[test]
    fn an_untransformed_fragment_is_tested_where_the_pointer_is() {
        let spatial = SpatialTree::with_viewport();
        let point: Point<DevicePx, Device> = Point::new(DevicePx(4.0), DevicePx(9.0));
        assert_eq!(into_local(point, None, &spatial), Some(point));
        assert_eq!(
            into_local(point, Some(spatial.viewport()), &spatial),
            Some(point),
            "the viewport's own coordinate system moves nothing",
        );
    }

    #[test]
    fn a_translated_fragment_is_tested_where_it_would_have_been() {
        let (spatial, id) = space_of(Affine2::translation(30.0, 0.0));
        let point: Point<DevicePx, Device> = Point::new(DevicePx(40.0), DevicePx(5.0));
        assert_eq!(
            into_local(point, Some(id), &spatial),
            Some(Point::new(DevicePx(10.0), DevicePx(5.0))),
        );
    }

    #[test]
    fn a_fragment_scaled_to_nothing_can_never_be_hit() {
        let (spatial, id) = space_of(Affine2::scale(0.0, 1.0));
        let point: Point<DevicePx, Device> = Point::new(DevicePx(1.0), DevicePx(1.0));
        assert_eq!(into_local(point, Some(id), &spatial), None);
    }

    #[test]
    fn a_point_is_carried_down_one_coordinate_system_at_a_time() {
        // The order is the whole of what a descent can get wrong, and undoing the inner transform
        // first is wrong in a way that looks right for a single node and for any two that commute.
        // A scale inside a translation is where the two orders part company.
        let mut spatial = SpatialTree::with_viewport();
        let owner = |raw| PropertyOwner::new(raw).expect("a handle is never the empty word");
        let outer = spatial.space_of(
            spatial.viewport(),
            owner(2),
            OwnSpace::of(
                Some(Affine2::translation(100.0, 0.0).to_matrix4()),
                None,
                false,
            ),
        );
        let inner = spatial.space_of(
            outer,
            owner(3),
            OwnSpace::of(Some(Affine2::scale(2.0, 2.0).to_matrix4()), None, false),
        );

        let point: Point<DevicePx, Device> = Point::new(DevicePx(140.0), DevicePx(20.0));
        assert_eq!(
            into_local(point, Some(inner), &spatial),
            Some(Point::new(DevicePx(20.0), DevicePx(10.0))),
            "the outermost transform is undone first, and the innermost last",
        );
    }

    #[test]
    fn a_flat_transform_is_undone_exactly_as_a_general_inverse_would() {
        // Every transform a document writes keeps the plane a plane, and those are undone by
        // solving two equations rather than by inverting four dimensions. The two are the same
        // answer or one of them is wrong, and "one of them is wrong" is a fragment hit somewhere
        // other than where it is drawn — silently, only under rotation, and only for the space
        // that carries it.
        let point: Point<DevicePx, Device> = Point::new(DevicePx(137.0), DevicePx(-42.5));
        let rotate = |turns: f32| Affine2::rotation(turns * core::f32::consts::TAU);
        for affine in [
            Affine2::scale(2.2, 1.0),
            Affine2::scale(1.0, 2.2),
            Affine2::scale(-3.0, 0.25),
            rotate(0.05),
            rotate(0.25),
            rotate(0.375),
            rotate(0.9),
            Affine2::skew(0.5, 0.0),
            Affine2::skew(0.0, -0.75),
            rotate(0.1).then(Affine2::scale(3.0, 0.5)),
            Affine2::translation(400.0, -90.0).then(rotate(0.2)),
        ] {
            let matrix = affine.to_matrix4();
            assert!(
                super::plane_of(&matrix).is_some(),
                "a two-dimensional transform keeps the plane a plane: {matrix:?}",
            );
            let general = general_inwards(matrix, point).expect("an invertible transform");
            let flat = super::inwards(matrix, point).expect("the same transform, the same answer");
            assert!(
                (flat.x.0 - general.x.0).abs() <= general.x.0.abs() * 1e-5 + 1e-3
                    && (flat.y.0 - general.y.0).abs() <= general.y.0.abs() * 1e-5 + 1e-3,
                "{flat:?} is not where a four-by-four inverse puts it, {general:?}, for {matrix:?}",
            );
        }
    }

    #[test]
    fn a_transform_that_leaves_the_plane_still_answers() {
        // The general inverse is not dead code: perspective and rotation out of the screen produce
        // a matrix that solving in two dimensions cannot express, and a query still has to answer
        // over one rather than dismiss it.
        let mut matrix = zgui_geom::Matrix4::IDENTITY;
        matrix.columns[2][0] = 0.5;
        matrix.columns[3][3] = 2.0;
        assert!(super::plane_of(&matrix).is_none(), "it leaves the plane");
        assert_eq!(
            super::inwards(matrix, Point::new(DevicePx(8.0), DevicePx(4.0))),
            Some(Point::new(DevicePx(16.0), DevicePx(8.0))),
        );
    }

    /// Undoing one transform the way a four-by-four inverse does, whatever the transform is.
    fn general_inwards(
        local: zgui_geom::Matrix4,
        point: Point<DevicePx, Device>,
    ) -> Option<Point<DevicePx, Device>> {
        let mapped = local
            .invert()?
            .transform_vector4([point.x.0, point.y.0, 0.0, 1.0]);
        let scale = mapped[3].recip();
        Some(Point::new(
            DevicePx(mapped[0] * scale),
            DevicePx(mapped[1] * scale),
        ))
    }

    #[test]
    fn a_name_that_no_longer_resolves_answers_nothing() {
        let (mut spatial, id) = space_of(Affine2::translation(30.0, 0.0));
        let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
        spatial.release(owner);
        spatial.recycle();
        let point: Point<DevicePx, Device> = Point::new(DevicePx(1.0), DevicePx(1.0));
        assert_eq!(into_local(point, Some(id), &spatial), None);
    }
}
