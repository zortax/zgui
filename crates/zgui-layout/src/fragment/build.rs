//! Turning one box's layout result into the piece of geometry everything downstream reads.
//!
//! Three things happen here at once, and they happen at once on purpose. The parent's absolute
//! origin is composed with the box's own offset; the result is put on the device pixel grid; and
//! the scroll and sticky offsets that move a box without moving anything around it are added in.
//! Running those as three passes would walk the same boxes three times and compute the same
//! cumulative origin twice, and the rounding rule — *round the cumulative absolute edges, and take
//! each size as the difference between two rounded edges* — needs that cumulative origin anyway.
//!
//! # Which space a fragment's rectangles are in
//!
//! Every rectangle a fragment carries is in *local* space: absolute layout coordinates with no
//! transform applied, neither its own nor any ancestor's. [`Fragment::transform`](crate::Fragment::transform) names the matrix
//! that maps that space onto the device, and a clip link recorded here is in the same local space
//! as the content it clips, so a clip inside a rotated box rotates with it rather than being
//! flattened to a rectangle.
//!
//! [`Fragment::ink`](crate::Fragment::ink), by contrast, is in **device** space, because it is what damage is computed
//! from and damage is measured in real pixels. A transformed box's ink is therefore the
//! axis-aligned bounds of its transformed painting area, which is what has to be redrawn when it
//! moves.

use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Edges, Point, Rect, Size};
use zgui_scene::{
    ClipId, ClipTable, OwnSpace, PropertyOwner, ScrollFrameId, SpatialId, SpatialTree,
    StackingContextId,
};

use crate::fragment::{FragmentFlags, anchored, clip, filter, ink, sticky, transform};
use crate::round::snap;
use crate::style::DeviceStyle;
use crate::tree::store::LayoutStore;

/// What a box inherits from the box that contains it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Descent {
    /// The parent border box's absolute origin, exactly as layout placed it.
    pub(crate) layout: (f32, f32),
    /// The same origin after rounding, which is what a child's snapped offset is measured from.
    pub(crate) rounded: (f32, f32),
    /// Everything the scroll offsets and sticky shifts above this box add up to.
    pub(crate) shift: (f32, f32),
    /// The clip chain this box is drawn under.
    pub(crate) clip: ClipId,
    /// The accumulated matrix mapping this box's local space onto the device, if it is not the
    /// identity. Carried as the matrix rather than as its name so a child composes onto it without
    /// walking the tree.
    pub(crate) matrix: Option<zgui_geom::Matrix4>,
    /// The coordinate system this box's content is drawn in.
    pub(crate) spatial: SpatialId,
    /// The stacking context this box is painted in.
    pub(crate) stacking: Option<StackingContextId>,
    /// The scrollable region this box moves with.
    pub(crate) scroll: Option<ScrollFrameId>,
    /// The nearest ancestor scrollport, in local space, which sticky positioning is resolved in.
    pub(crate) scrollport: Rect<DevicePx, Device>,
    /// The containing block's content box, in local space, which a sticky shift is clamped to.
    pub(crate) containing: Rect<DevicePx, Device>,
    /// Whether [`Descent::layout`] and [`Descent::rounded`] are what this box's descendants were
    /// last composed against.
    ///
    /// False as soon as any box on the way down reports a layout result its standing fragments were
    /// not composed from, because the rounded origin every descendant is snapped against is then a
    /// different one and no descendant has merely moved.
    pub(crate) layout_stable: bool,
}

impl Descent {
    /// The state a document's root box is composed against.
    pub(crate) fn root(viewport: Size<DevicePx, Device>, spatial: SpatialId) -> Self {
        let viewport = Rect::new(Point::new(DevicePx(0.0), DevicePx(0.0)), viewport);
        Self {
            layout: (0.0, 0.0),
            rounded: (0.0, 0.0),
            shift: (0.0, 0.0),
            clip: ClipId::ROOT,
            matrix: None,
            spatial,
            stacking: None,
            scroll: None,
            scrollport: viewport,
            containing: viewport,
            layout_stable: true,
        }
    }
}

/// One box's composed geometry, and what its children inherit.
#[derive(Clone, Debug)]
pub(crate) struct Placed {
    /// The border box in local space.
    pub(crate) border_box: Rect<DevicePx, Device>,
    /// The padding box in local space.
    pub(crate) padding_box: Rect<DevicePx, Device>,
    /// The content box in local space.
    pub(crate) content_box: Rect<DevicePx, Device>,
    /// The snapped border widths.
    pub(crate) border: Edges<DevicePx>,
    /// The snapped padding widths.
    pub(crate) padding: Edges<DevicePx>,
    /// Everything this box paints, in device space.
    pub(crate) ink: Rect<DevicePx, Device>,
    /// The same, in the box's own space.
    pub(crate) local_ink: Rect<DevicePx, Device>,
    /// What painting, damage and hit testing branch on.
    pub(crate) flags: FragmentFlags,
    /// The clip chain this box itself is drawn under.
    pub(crate) clip: ClipId,
    /// The coordinate system that chain's rectangles were measured in.
    pub(crate) clip_transform: Option<SpatialId>,
    /// The coordinate system this box's own content is drawn in.
    pub(crate) transform: Option<SpatialId>,
    /// A fingerprint of what that coordinate system currently resolves to.
    ///
    /// The name is structural and survives the matrix under it being rewritten, which is the whole
    /// point of naming a coordinate system after the box that establishes it — and it means the
    /// name alone cannot tell a comparison that this box is drawn through a different matrix than
    /// it was. This is what tells it.
    pub(crate) transform_hash: u64,
    /// The stacking context this box belongs to.
    pub(crate) stacking: Option<StackingContextId>,
    /// The scrollable region it moves with.
    pub(crate) scroll: Option<ScrollFrameId>,
    /// Whether this box reads pixels outside every rectangle it writes.
    pub(crate) reads_outside: bool,
    /// Whether this box blends with what is painted behind it.
    pub(crate) blends: bool,
    /// The layout result after snapping, which is what the box's own record keeps.
    pub(crate) snapped: taffy::Layout,
    /// Whether this box itself moves by the same vector as the box above it.
    ///
    /// See [`Fragment::subtree_rigid`](crate::Fragment::subtree_rigid) for the three styles that
    /// answer no and why each of them does.
    pub(crate) rigid: bool,
    /// What this box's children are composed against.
    pub(crate) descent: Descent,
}

/// Everything composing a fragment needs that is not in the box tree.
///
/// The clip table and the spatial tree are borrowed rather than owned because their identifiers
/// outlive the frame: a fragment replayed from a previous frame's recorded painting carries *that*
/// frame's names, so storage rebuilt per frame would draw one fragment with another's clip.
pub struct Tables<'a> {
    /// Where clip chains are interned.
    pub clips: &'a mut ClipTable,
    /// Where coordinate systems are named.
    pub spatial: &'a mut SpatialTree,
    /// The two numbers no style carries.
    pub device: DeviceStyle,
    /// Where each scroll container is scrolled to.
    pub scroll: &'a crate::scroll_region::ScrollOffsets,
    /// Where an animation is putting each element it is moving, sorted by element.
    ///
    /// Read for one question and one only — the matrix a box is drawn under. An animation that
    /// moves a transform an element already has changes nothing else about the box, so the shared
    /// style goes on answering everything else; an animation that would change anything else is
    /// refused this table by the stage that fills it and cascades instead. Empty for a frame with
    /// nothing animating, which is nearly all of them.
    pub placements: &'a [(zgui_dom::NodeKey, zgui_dom::side::AnimPlacement)],
}

/// Composes one box against its parent.
pub(crate) fn place(
    store: &LayoutStore,
    tables: &mut Tables<'_>,
    key: BoxKey,
    from: Descent,
) -> Option<Placed> {
    let node = store.get(key)?;
    let state = store.state(key)?;
    let unrounded = state.unrounded;
    // Read before the walk overwrites it: what the descendants were snapped against is only the
    // same origin if this box's own result is the one they were composed from.
    let layout_stable = from.layout_stable && state.composed == unrounded;
    let scale = tables.device.scale;

    let (snapped, edges) = snap::place(unrounded, from.layout, from.rounded);
    let layout = (
        from.layout.0 + unrounded.location.x,
        from.layout.1 + unrounded.location.y,
    );
    let rounded = (edges.left, edges.top);

    // A `position: fixed` box is in the viewport rather than in whatever is scrolling around it,
    // so it takes none of the shift the scroll containers above it accumulated — and neither does
    // anything inside it, which is why the shift is dropped here rather than only for this box.
    let inherited = if anchored::ignores_scroll(&node.style) {
        (0.0, 0.0)
    } else {
        from.shift
    };

    let flow = Rect::from_corners(
        Point::new(
            DevicePx(edges.left + inherited.0),
            DevicePx(edges.top + inherited.1),
        ),
        Point::new(
            DevicePx(edges.right + inherited.0),
            DevicePx(edges.bottom + inherited.1),
        ),
    );

    let is_sticky = node.style.get_box().position == zgui_css::values::size::PositionValue::Sticky;
    // Sticky is resolved here rather than in layout because nothing around the box moves: the
    // shift is added to the composed position and to nothing else.
    let (sticky_x, sticky_y) =
        sticky::offset(&node.style, flow, from.scrollport, from.containing, scale);
    let shift = (inherited.0 + sticky_x, inherited.1 + sticky_y);
    let border_box = flow.translate(Size::new(DevicePx(sticky_x), DevicePx(sticky_y)));

    let border = snap::edges(snapped.border);
    let padding = snap::edges(snapped.padding);
    let padding_box = border_box.inset(border);
    let scrollbar: Size<DevicePx, Device> = Size::new(
        DevicePx(snapped.scrollbar_size.width),
        DevicePx(snapped.scrollbar_size.height),
    );
    let inner = padding_box.inset(padding);
    let content_box = Rect::new(
        inner.origin,
        Size::new(
            DevicePx((inner.size.width.0 - scrollbar.width.0).max(0.0)),
            DevicePx((inner.size.height.0 - scrollbar.height.0).max(0.0)),
        ),
    );

    // The placement an animation wrote, if one is moving this element, and the box's own style
    // otherwise. Nothing else in this function consults it: whether the box establishes a stacking
    // context, whether it is a containing block and what it is painted in are all read from the
    // shared style, and an animation that would move any of those never reaches this table.
    let placed = transform::animated::of(tables.placements, node.source);
    let box_ = placed.map_or_else(
        || node.style.get_box(),
        zgui_dom::side::AnimPlacement::group,
    );
    let own_matrix = transform::matrix_of(box_, border_box, scale);
    let matrix = match (from.matrix, own_matrix) {
        (None, None) => None,
        (Some(outer), None) => Some(outer),
        (None, Some(own)) => Some(own),
        (Some(outer), Some(own)) => Some(own.then(&outer)),
    };

    // The three inputs that decide whether this box moves by the vector the box above it moved by,
    // asked once. `None` is the overwhelming majority and means the box is drawn in the coordinate
    // system above it and takes its name for it; `rigid` below is that same answer read back, not a
    // second fold of the same three styles.
    // A sticky box with nothing scrollable above it is held still against the window, which is a
    // scrollable region without being a box; without that, a box whose stickiness has no named port
    // would answer that it moves with everything around it, which is the one thing sticky is not.
    let sticky_port = is_sticky.then(|| from.scroll.unwrap_or(zgui_scene::ScrollFrameId::VIEWPORT));
    let own_space = OwnSpace::of(
        own_matrix,
        sticky_port,
        anchored::ignores_scroll(&node.style),
    );
    let space = tables
        .spatial
        .space_of(from.spatial, PropertyOwner::of(key), own_space);
    let transform = Some(space);
    let transform_hash = {
        use zgui_scene::Content;
        matrix
            .unwrap_or(zgui_geom::Matrix4::IDENTITY)
            .content_hash()
    };

    let local_ink = ink::of(&node.style, border_box, scale);
    let device_ink = match matrix {
        Some(matrix) => transform::transformed_bounds(&matrix, local_ink),
        None => local_ink,
    };

    // The chain this box is drawn under was measured in the space of whichever ancestor imposed it,
    // which is the space this box's *parent* laid out in — this box's own transform is not applied
    // to it. Interned only when there is a chain to place, because a document's boxes are
    // overwhelmingly unclipped.
    let clip_transform = if from.clip.is_root() {
        None
    } else {
        Some(from.spatial)
    };
    let child_clip = clip::chain_for_children(
        tables.clips,
        from.clip,
        &node.style,
        padding_box,
        border,
        scale,
        Size::new(DevicePx(shift.0), DevicePx(shift.1)),
        // The box's own space, transform and all: a transform on a clipping box carries its clip
        // with it, which is the same matrix its own fragments are drawn through.
        space,
        PropertyOwner::of(key),
    );
    let clips_children = child_clip != from.clip;

    let stacking = if crate::fragment::stacking::establishes(store, key) {
        Some(crate::fragment::stacking::id_of(key))
    } else {
        from.stacking
    };

    let scrolls = crate::scroll_region::is_scroll_container(&node.style);
    let scroll = if scrolls {
        Some(ScrollFrameId(key.index()))
    } else {
        from.scroll
    };
    let scrollport = if scrolls {
        padding_box
    } else {
        from.scrollport
    };
    let offset = if scrolls {
        node.source
            .map_or(Point::new(DevicePx(0.0), DevicePx(0.0)), |element| {
                tables.scroll.of(element)
            })
    } else {
        Point::new(DevicePx(0.0), DevicePx(0.0))
    };

    let rigid = own_space.is_none();

    let mut flags = FragmentFlags::EMPTY;
    if clips_children {
        flags = flags.union(FragmentFlags::CLIPS_CHILDREN);
    }
    if own_matrix.is_some() {
        flags = flags.union(FragmentFlags::HAS_TRANSFORM);
    }
    if Some(crate::fragment::stacking::id_of(key)) == stacking {
        flags = flags.union(FragmentFlags::IS_STACKING_CONTEXT);
    }
    if is_sticky {
        flags = flags.union(FragmentFlags::IS_STICKY);
    }

    Some(Placed {
        border_box,
        padding_box,
        content_box,
        border,
        padding,
        ink: device_ink,
        local_ink,
        flags,
        clip: from.clip,
        clip_transform,
        transform,
        transform_hash,
        stacking,
        scroll,
        reads_outside: filter::reads_outside(&node.style, scale),
        blends: node.style.get_effects().mix_blend_mode
            != zgui_css::values::effect::MixBlendModeValue::Normal,
        snapped,
        rigid,
        descent: Descent {
            layout,
            rounded,
            shift: (shift.0 - offset.x.0, shift.1 - offset.y.0),
            clip: child_clip,
            matrix,
            spatial: space,
            stacking,
            scroll,
            scrollport,
            containing: content_box,
            layout_stable,
        },
    })
}

/// The rectangle one line of an inline formatting context occupies, in the same local space as the
/// box that holds it.
///
/// The lines themselves are resolved while the box is laid out; where they end up is a question
/// about the box's absolute position, which is answered here and only here.
pub(crate) fn line_rect(
    content_box: Rect<DevicePx, Device>,
    line: &crate::inline::lines::LineBox,
) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(
            DevicePx(content_box.origin.x.0 + line.offset),
            DevicePx(content_box.origin.y.0 + line.top),
        ),
        Size::new(DevicePx(line.width), DevicePx(line.height())),
    )
}
