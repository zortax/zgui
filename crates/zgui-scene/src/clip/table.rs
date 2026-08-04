//! The clip trie, and the questions asked of it.

use zgui_geom::{Corners, Device, DevicePx, Matrix4, Point, Rect, Size, Vec2};

/// A translation of no distance, which is what a link that nothing has carried holds.
const UNMOVED: Size<DevicePx, Device> = Size::ZERO;

use crate::clip::link::{ClipLink, ClipNode};
use crate::clip::resolved::{ResolvedClip, RoundedTest};
use crate::id::ClipId;
use crate::spatial::SpatialId;
use crate::table::Table;

/// Every clip chain in the document, interned so that chains sharing an outer prefix share nodes.
pub type ClipTable = Table<ClipId, ClipNode>;

impl ClipTable {
    /// How many rounded-corner tests one draw call can apply.
    ///
    /// A chain needing more is promoted into a target of its own rather than truncated. Two is what
    /// a shader can evaluate without the per-fragment cost becoming the point, and it covers the
    /// nesting real documents produce — a rounded card inside a rounded scrollport.
    pub const MAX_INLINE_ROUNDED: usize = 2;

    /// A table holding only [`ClipId::ROOT`], pinned so its id can never be reused.
    pub fn rooted() -> Self {
        let mut table = Self::new();
        let root = table.intern(ClipNode::Root);
        debug_assert_eq!(root, ClipId::ROOT, "the root chain must land at index zero");
        table.pin(root);
        table
    }

    /// The chain that is `parent` with one more link applied inside it.
    ///
    /// For a link nothing has carried anywhere. A clipping box that scrolls has been carried, and
    /// wants [`ClipTable::push_shifted`] so that the chain it issues while it moves is the chain it
    /// issued before it started.
    pub fn push(&mut self, parent: ClipId, link: ClipLink) -> ClipId {
        self.push_shifted(parent, link, UNMOVED)
    }

    /// The same, for a link whose rectangle has been carried `shift` from where layout placed it.
    ///
    /// `shift` is everything the scroll and sticky offsets above the clipping box add up to. It is
    /// what the chain is *named* by — the rectangle less the shift is the same rectangle of the
    /// document whatever the document is scrolled to — while the rectangle the chain resolves to
    /// stays the one being drawn.
    pub fn push_shifted(
        &mut self,
        parent: ClipId,
        link: ClipLink,
        shift: Size<DevicePx, Device>,
    ) -> ClipId {
        self.intern(ClipNode::Link {
            link,
            parent,
            shift,
        })
    }

    /// The chain that is [`ClipId::ROOT`] with one link applied.
    pub fn only(&mut self, link: ClipLink) -> ClipId {
        self.push(ClipId::ROOT, link)
    }

    /// How many links `id` applies.
    pub fn depth(&self, id: ClipId) -> u32 {
        let mut depth = 0;
        let mut cursor = id;
        while let Some(ClipNode::Link { parent, .. }) = self.get(cursor) {
            depth += 1;
            cursor = *parent;
        }
        depth
    }

    /// The chain's links, outermost first.
    pub fn links(&self, id: ClipId) -> Vec<ClipLink> {
        let mut links = Vec::new();
        let mut cursor = id;
        while let Some(ClipNode::Link { link, parent, .. }) = self.get(cursor) {
            links.push(*link);
            cursor = *parent;
        }
        links.reverse();
        links
    }

    /// The chain flattened into what a single draw call binds.
    ///
    /// Links beyond what one draw call can apply are *ignored here* and reported by
    /// [`ClipTable::needs_group_target`] instead: silently applying the first two rounded tests of
    /// three would be a wrong pixel with no error, so the overflow is a question the caller has to
    /// have asked.
    ///
    /// Every link is read as it was interned, so a link measured in a transformed coordinate
    /// system stays in that system's coordinates. A caller comparing against device pixels — which
    /// is every caller applying the clip rather than naming it — wants
    /// [`ClipTable::resolve_placed`] with the frame's matrices instead.
    pub fn resolve(&self, id: ClipId) -> ResolvedClip {
        self.resolve_placed(id, &|_| None)
    }

    /// The chain flattened into device pixels, each link put where its coordinate system now is.
    ///
    /// A clipping box inside a transformed subtree interned its rectangle in its own space, and
    /// the space moves without the link being interned again — a dialog holding the placement its
    /// entrance settled on is moved by writing its coordinate system. What applies the clip asks
    /// in device pixels, so each link is resolved through `matrix_of` here, at the moment of use,
    /// against the same matrices the content is drawn through. A link whose space answers nothing
    /// is read where it was interned, which is exact for everything untransformed.
    ///
    /// A transformed rectangle is not a rectangle, so what a turned or skewed link resolves to is
    /// the smallest upright box containing it, its radii scaled by the axes' own lengths — a clip
    /// slightly too generous rather than one that cuts content the transform kept inside it. For
    /// the translations and scales interfaces are placed with, the resolution is exact.
    pub fn resolve_placed(
        &self,
        id: ClipId,
        matrix_of: &dyn Fn(SpatialId) -> Option<Matrix4>,
    ) -> ResolvedClip {
        let mut resolved = ResolvedClip::unbounded();
        let mut rounded = 0usize;
        // Walk towards the root instead of materialising and reversing the chain. Intersections
        // commute; rounded tests are shifted down so the two outermost survive; and the first mask
        // seen is the innermost one, which is the one the former outer-to-inner overwrite kept.
        let mut cursor = id;
        loop {
            let Some(node) = self.get(cursor) else {
                break;
            };
            let ClipNode::Link { link, parent, .. } = node else {
                break;
            };
            let link = *link;
            match link {
                ClipLink::RoundedRect { rect, radii, space } => {
                    let (rect, radii) = placed(rect, radii, matrix_of(space));
                    intersect(&mut resolved.aabb, rect);
                    if link.is_rounded() {
                        let test = RoundedTest {
                            rect: [
                                rect.origin.x.0,
                                rect.origin.y.0,
                                rect.size.width.0,
                                rect.size.height.0,
                            ],
                            radii: [
                                radii.top_left.x.0,
                                radii.top_left.y.0,
                                radii.top_right.x.0,
                                radii.top_right.y.0,
                                radii.bottom_right.x.0,
                                radii.bottom_right.y.0,
                                radii.bottom_left.x.0,
                                radii.bottom_left.y.0,
                            ],
                        };
                        if rounded == 0 {
                            resolved.rounded[0] = test;
                            rounded = 1;
                        } else {
                            resolved.rounded[1] = resolved.rounded[0];
                            resolved.rounded[0] = test;
                            rounded = Self::MAX_INLINE_ROUNDED;
                        }
                    }
                }
                ClipLink::Mask { tile, .. } => {
                    if resolved.mask.is_none() {
                        resolved.mask = Some(tile);
                    }
                }
            }
            cursor = *parent;
        }
        resolved.rounded_count = rounded as u32;
        resolved
    }

    /// Whether `id` is deeper than one draw call can apply, and so needs its content drawn into a
    /// target of its own.
    pub fn needs_group_target(&self, id: ClipId) -> bool {
        let mut rounded = 0;
        let mut masks = 0;
        let mut cursor = id;
        while let Some(ClipNode::Link { link, parent, .. }) = self.get(cursor) {
            rounded += usize::from(link.is_rounded());
            masks += usize::from(matches!(link, ClipLink::Mask { .. }));
            cursor = *parent;
        }
        rounded > Self::MAX_INLINE_ROUNDED || masks > 1
    }

    /// The clip rectangle `id` admits, as geometry rather than as shader input.
    ///
    /// Each link is read as it was interned; see [`ClipTable::resolve`]. A caller intersecting
    /// this with device-space geometry wants [`ClipTable::bounds_placed`].
    pub fn bounds(&self, id: ClipId) -> Rect<DevicePx, Device> {
        let resolved = self.resolve(id);
        Rect::new(
            Point::new(DevicePx(resolved.aabb[0]), DevicePx(resolved.aabb[1])),
            Size::new(DevicePx(resolved.aabb[2]), DevicePx(resolved.aabb[3])),
        )
    }

    /// The device pixels `id` admits, each link put where its coordinate system now is.
    ///
    /// The geometry reading of [`ClipTable::resolve_placed`], for a cull intersecting the clip
    /// with ink that is measured on the device.
    pub fn bounds_placed(
        &self,
        id: ClipId,
        matrix_of: &dyn Fn(SpatialId) -> Option<Matrix4>,
    ) -> Rect<DevicePx, Device> {
        let resolved = self.resolve_placed(id, matrix_of);
        Rect::new(
            Point::new(DevicePx(resolved.aabb[0]), DevicePx(resolved.aabb[1])),
            Size::new(DevicePx(resolved.aabb[2]), DevicePx(resolved.aabb[3])),
        )
    }

    /// The deepest chain that both `left` and `right` apply.
    ///
    /// Because chains are interned into a trie, this is a walk up two parent pointers and never a
    /// comparison of two lists. It is what a composite covering several items binds: one draw call
    /// applies one clip, so it must be one every item in it genuinely has.
    pub fn common_ancestor(&self, left: ClipId, right: ClipId) -> ClipId {
        let (mut left, mut right) = (left, right);
        let (mut left_depth, mut right_depth) = (self.depth(left), self.depth(right));
        while left_depth > right_depth {
            left = self.parent(left);
            left_depth -= 1;
        }
        while right_depth > left_depth {
            right = self.parent(right);
            right_depth -= 1;
        }
        while left != right {
            left = self.parent(left);
            right = self.parent(right);
        }
        left
    }

    /// The part of `id` that lies **below** `ancestor`, as a chain of its own.
    ///
    /// Empty — that is, [`ClipId::ROOT`] — when `id` is `ancestor`, which is the common case and
    /// costs nothing. Anything else is what has to be applied inside whatever draws the content,
    /// because the draw call itself is already bound to `ancestor`.
    pub fn residual(&mut self, id: ClipId, ancestor: ClipId) -> ClipId {
        if id == ancestor {
            return ClipId::ROOT;
        }
        let below = self.links_below(id, ancestor);
        let mut residual = ClipId::ROOT;
        for (link, shift) in below {
            residual = self.push_shifted(residual, link, shift);
        }
        residual
    }

    /// Whether every link of `id` can be applied inside a vector rasteriser's own scene.
    ///
    /// When it cannot — a sampled raster mask — the content has to be composited through a pass
    /// bound to its own clip, which is the one case where a clip costs a whole pass.
    pub fn is_expressible_in_vector_scene(&self, id: ClipId) -> bool {
        self.links(id)
            .iter()
            .all(ClipLink::is_expressible_in_vector_scene)
    }

    /// The chain `id` is applied inside, or [`ClipId::ROOT`] when there is none.
    fn parent(&self, id: ClipId) -> ClipId {
        match self.get(id) {
            Some(ClipNode::Link { parent, .. }) => *parent,
            _ => ClipId::ROOT,
        }
    }

    /// The links of `id` that lie below `ancestor`, outermost first, each with how far it has been
    /// carried.
    ///
    /// The distance travels with the link because the chain built out of these has to be named the
    /// way the chain it was taken from is named — a residual re-derived from moved rectangles alone
    /// would be a new chain on every frame of a scroll.
    fn links_below(&self, id: ClipId, ancestor: ClipId) -> Vec<(ClipLink, Size<DevicePx, Device>)> {
        let mut links = Vec::new();
        let mut cursor = id;
        while cursor != ancestor {
            let Some(ClipNode::Link {
                link,
                parent,
                shift,
            }) = self.get(cursor)
            else {
                break;
            };
            links.push((*link, *shift));
            cursor = *parent;
        }
        links.reverse();
        links
    }
}

/// One link's rectangle and radii, put where its coordinate system's matrix says it is drawn.
///
/// The four corners go through the matrix and the upright box around them is taken, so a rotation
/// answers with the box its clip actually reaches rather than a rectangle that never turned. The
/// radii are scaled by the length each axis maps to, which is exact for every translation and
/// scale and a generous reading for a rotation — a clip slightly too admitting, never one that
/// cuts what the transform kept inside it.
fn placed(
    rect: Rect<DevicePx, Device>,
    radii: Corners<Vec2<DevicePx>>,
    matrix: Option<Matrix4>,
) -> (Rect<DevicePx, Device>, Corners<Vec2<DevicePx>>) {
    let Some(matrix) = matrix else {
        return (rect, radii);
    };
    let (x, y) = (rect.origin.x.0, rect.origin.y.0);
    let (width, height) = (rect.size.width.0, rect.size.height.0);
    let corners = [
        matrix.transform_point(x, y, 0.0),
        matrix.transform_point(x + width, y, 0.0),
        matrix.transform_point(x, y + height, 0.0),
        matrix.transform_point(x + width, y + height, 0.0),
    ];
    let left = corners
        .iter()
        .map(|point| point[0])
        .fold(f32::MAX, f32::min);
    let right = corners
        .iter()
        .map(|point| point[0])
        .fold(f32::MIN, f32::max);
    let top = corners
        .iter()
        .map(|point| point[1])
        .fold(f32::MAX, f32::min);
    let bottom = corners
        .iter()
        .map(|point| point[1])
        .fold(f32::MIN, f32::max);
    let moved = Rect::new(
        Point::new(DevicePx(left), DevicePx(top)),
        Size::new(DevicePx(right - left), DevicePx(bottom - top)),
    );
    // How long one unit of each axis is once mapped, which is what an ellipse radius scales by.
    let origin = matrix.transform_point(0.0, 0.0, 0.0);
    let unit_x = matrix.transform_point(1.0, 0.0, 0.0);
    let unit_y = matrix.transform_point(0.0, 1.0, 0.0);
    let scale_x = ((unit_x[0] - origin[0]).powi(2) + (unit_x[1] - origin[1]).powi(2)).sqrt();
    let scale_y = ((unit_y[0] - origin[0]).powi(2) + (unit_y[1] - origin[1]).powi(2)).sqrt();
    let scaled = |radius: Vec2<DevicePx>| {
        Vec2::new(
            DevicePx(radius.x.0 * scale_x),
            DevicePx(radius.y.0 * scale_y),
        )
    };
    let moved_radii = Corners {
        top_left: scaled(radii.top_left),
        top_right: scaled(radii.top_right),
        bottom_right: scaled(radii.bottom_right),
        bottom_left: scaled(radii.bottom_left),
    };
    (moved, moved_radii)
}

/// Narrows `aabb` to its intersection with `rect`, never past zero extent.
fn intersect(aabb: &mut [f32; 4], rect: Rect<DevicePx, Device>) {
    let left = aabb[0].max(rect.origin.x.0);
    let top = aabb[1].max(rect.origin.y.0);
    let right = (aabb[0] + aabb[2]).min(rect.origin.x.0 + rect.size.width.0);
    let bottom = (aabb[1] + aabb[3]).min(rect.origin.y.0 + rect.size.height.0);
    *aabb = [left, top, (right - left).max(0.0), (bottom - top).max(0.0)];
}
