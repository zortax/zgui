//! One painted piece of one box.
//!
//! Every stage after layout reads fragments and never the layout engine's own results. That single
//! rule is what lets the layout engine be replaced without touching painting, hit testing or
//! accessibility.
//!
//! A box produces one fragment per piece it is broken into: one per line for inline content, one
//! per column, one per page. Boxes are what the layout algorithms see; fragments are what
//! everything downstream sees.

mod anchored;
pub mod build;
pub mod clip;
pub mod corner;
pub mod custom_properties;
pub mod diff;
pub mod filter;
pub mod hit;
pub mod index;
pub mod ink;
pub mod scroll_nodes;
pub mod stacking;
pub mod sticky;
pub mod transform;

use smallvec::SmallVec;
use zgui_arena::Key;
use zgui_dom::NodeKey;
use zgui_dom::host::ReplacedId;
use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Edges, Rect, Size};
use zgui_scene::{ClipId, ScrollFrameId, SpatialId, StackingContextId};

use crate::axis::Axis;

/// A generation-checked name for one fragment.
pub type FragKey = Key<Fragment>;

/// The fragments one box produced, in order.
pub type FragList = SmallVec<[FragKey; 1]>;

/// A shaped paragraph, named by the store that holds it.
///
/// The identifier is opaque here: what it resolves to is a shaping result, and the only thing a
/// fragment does with it is hand it back to whoever asked which line of which paragraph this
/// fragment draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParagraphId(pub u32);

impl ParagraphId {
    /// The identifier's numeric value, for indexing and for transcripts.
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// Which piece of a scrollbar a fragment draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScrollbarPart {
    /// The groove the thumb runs in.
    Track,
    /// The draggable thumb.
    Thumb,
}

/// What a fragment draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FragmentKind {
    /// A box's own background, borders and decorations.
    Box,
    /// One line of a paragraph.
    Line {
        /// The paragraph the line belongs to.
        paragraph: ParagraphId,
        /// Which line of it, counting from zero.
        line: u16,
    },
    /// One style-uniform run of glyphs within a line.
    TextRun {
        /// The paragraph the run belongs to.
        paragraph: ParagraphId,
        /// Which run of it, counting from zero.
        run: u16,
    },
    /// Content this engine does not lay out: an image, a video, an embedded surface.
    Replaced {
        /// What to draw.
        content: ReplacedId,
    },
    /// Outlines the element carries, drawn inside its content box.
    ///
    /// The outlines themselves are not named here. They are properties of the element and can
    /// change without any box or any piece of geometry changing — an icon swapped for another of
    /// the same size is the same rectangle drawn differently — so what a drawing draws is asked of
    /// the element at the moment it is painted, and this kind says only that there is one.
    Vector,
    /// Content a registered custom element paints, for the same reason as [`FragmentKind::Vector`]
    /// unnamed here: what it draws is asked of the element's implementation at painting time, and
    /// this kind says only that there is one.
    Custom,
    /// Part of a scrollbar.
    Scrollbar {
        /// Which axis it runs along.
        axis: Axis,
        /// Which piece of it this is.
        part: ScrollbarPart,
    },
}

impl FragmentKind {
    /// Whether two kinds name the same piece of a box, as opposed to holding the same content.
    ///
    /// A fragment's name is what the hit index, its recorded painting and the previous frame's
    /// damage all refer to, so a piece that is still the same piece must keep it. What makes that
    /// a different question from equality is the paragraph a line belongs to: a paragraph is
    /// interned by the shaping of its characters, so typing one character issues a new identifier
    /// for it — and a line matched on equality would then be destroyed and remade on every
    /// keystroke, which unregisters its hit entry, discards its paint record and forces the
    /// painting order to be derived again for the whole document.
    ///
    /// *Which* paragraph a line draws is still part of what it draws, so a caller that reuses a
    /// name across a change of paragraph owes the fragment a repaint. That is a separate obligation
    /// from the name, and keeping the two apart is the point.
    ///
    /// ```
    /// use zgui_layout::fragment::{FragmentKind, ParagraphId};
    ///
    /// let before = FragmentKind::Line { paragraph: ParagraphId(0), line: 1 };
    /// let after = FragmentKind::Line { paragraph: ParagraphId(7), line: 1 };
    /// assert!(before.same_piece(after));
    /// assert!(!before.same_piece(FragmentKind::Line { paragraph: ParagraphId(0), line: 2 }));
    /// assert!(!before.same_piece(FragmentKind::Box));
    /// ```
    pub fn same_piece(self, other: Self) -> bool {
        match (self, other) {
            (Self::Line { line, .. }, Self::Line { line: other, .. }) => line == other,
            (Self::TextRun { run, .. }, Self::TextRun { run: other, .. }) => run == other,
            (this, other) => this == other,
        }
    }
}

/// Properties of a fragment that painting, damage and hit testing branch on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FragmentFlags(u16);

impl FragmentFlags {
    /// Nothing set.
    pub const EMPTY: Self = Self(0);
    /// This fragment clips its descendants to its padding box.
    pub const CLIPS_CHILDREN: Self = Self(1 << 0);
    /// This fragment establishes a stacking context.
    pub const IS_STACKING_CONTEXT: Self = Self(1 << 1);
    /// This fragment carries a transform.
    pub const HAS_TRANSFORM: Self = Self(1 << 2);
    /// This fragment is positioned stickily.
    pub const IS_STICKY: Self = Self(1 << 3);
    /// This fragment reads pixels outside every rectangle it writes, and is listed in the
    /// read-extent registry.
    pub const HAS_READ_EXTENT: Self = Self(1 << 4);
    /// Somewhere below this fragment is one that blends with what is behind it, so this subtree
    /// cannot be flattened into its parent's paint.
    pub const HAS_BLENDING_DESCENDANT: Self = Self(1 << 5);
    /// The box's own painting is nothing: no background, no border, no outline, no shadow.
    ///
    /// Decided from the style when the fragment is placed, and read where a change to the box is
    /// weighed for damage — a box that paints nothing owes none for a move of its own.
    pub const PAINTS_NOTHING: Self = Self(1 << 6);
    /// A line that paints the effects of the nested inline boxes on it: their backgrounds,
    /// borders, decorations, text shadows or baseline shifts.
    ///
    /// Read where damage is weighed: a style change on one of those boxes repaints the line.
    pub const INLINE_EFFECTS: Self = Self(1 << 7);
    /// This fragment filters what is painted behind it: it has a `backdrop-filter`.
    pub const FILTERS_BACKDROP: Self = Self(1 << 8);
    /// A fragment strictly below this one reads what is painted behind it: it blends with it or
    /// filters it.
    ///
    /// Read where a box decides whether it needs a target of its own: a backdrop root isolates
    /// only for such a reader.
    pub const READS_BACKDROP_BELOW: Self = Self(1 << 9);
    /// The box applies an effect to everything below it: an opacity below one, a filter or a
    /// blend mode, from its style or from a running animation.
    ///
    /// Read where damage is weighed: a repaint of such a box changes every pixel its subtree
    /// paints. Its own ink does not hold the pixels the subtree paints outside it, which are all
    /// of them when the box paints nothing.
    pub const GROUPS_SUBTREE: Self = Self(1 << 10);
    /// This scroll container draws its children in a coordinate system of its own, which its
    /// scroll offset moves.
    ///
    /// Set when the box is placed. Read where the box is placed again: a box that starts or stops
    /// doing so hands its children another coordinate system, so they are composed again.
    pub const SCROLLS_CONTENT: Self = Self(1 << 11);
    /// This box or one below it is sticky against the scrollport of a box above this one.
    ///
    /// Folded on the unwind and stopped at a scroll container, which is the port of the sticky
    /// boxes below it. A scroll that writes only a content space visits the boxes that carry it,
    /// because their shift is measured against a port the content moves past.
    pub const STICKY_BELOW: Self = Self(1 << 12);
    /// This box or one below it is `position: fixed`.
    ///
    /// Folded on the unwind and stopped at the fixed box. A scroll that writes only a content space
    /// visits the boxes that carry it: a fixed box stays where it is while the content moves, so
    /// the subtree inks above it are folded again.
    pub const FIXED_BELOW: Self = Self(1 << 13);
    /// A fixed box at or below this one hangs from a transformed or sticky box that a scroll
    /// moves.
    ///
    /// Folded on the unwind. A scroll container above it keeps its children in its own space,
    /// because a content space moves the transformed or sticky box and with it the fixed box.
    pub const FIXED_UNDER_SCROLLED: Self = Self(1 << 14);
    /// A sticky box below this scroll container is sticky against its port.
    ///
    /// Written on the scroll container only, where [`FragmentFlags::STICKY_BELOW`] stops. A scroll
    /// that writes the container's content space visits its sticky boxes only when it is set.
    pub const STICKY_IN_PORT: Self = Self(1 << 15);
    /// The flags that say which boxes below a scroll depends on beyond its content space.
    pub const BOUND: Self = Self::STICKY_BELOW
        .union(Self::FIXED_BELOW)
        .union(Self::FIXED_UNDER_SCROLLED);
    /// The flags the unwind writes from the subtree, which a comparison of the box itself ignores.
    pub const FOLDED: Self = Self::HAS_BLENDING_DESCENDANT
        .union(Self::READS_BACKDROP_BELOW)
        .union(Self::BOUND)
        .union(Self::STICKY_IN_PORT);
    /// The flags a comparison of the box's geometry ignores.
    ///
    /// The subtree's flags, [`FragmentFlags::GROUPS_SUBTREE`] and
    /// [`FragmentFlags::SCROLLS_CONTENT`]. A change of the first owes the subtree's ink, which the
    /// repaint mark that comes with it already damages. A change of the second composes every
    /// child again, and the children damage what they draw.
    pub const UNCOMPARED: Self = Self::FOLDED
        .union(Self::GROUPS_SUBTREE)
        .union(Self::SCROLLS_CONTENT);
    /// The flags that say this fragment or one below it reads what is painted behind it.
    pub const READS_BACKDROP: Self = Self::HAS_BLENDING_DESCENDANT
        .union(Self::FILTERS_BACKDROP)
        .union(Self::READS_BACKDROP_BELOW);

    /// Whether every flag in `other` is set here.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether any flag in `other` is set here.
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// The union of two sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The flags set both here and in `other`.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// The same set with `other` removed.
    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// The raw bits, for transcripts.
    pub const fn bits(self) -> u16 {
        self.0
    }
}

/// One painted piece of one box, in absolute layout pixels.
///
/// The rectangles are device pixels only after a matrix is applied: [`Fragment::transform`] for the
/// border, padding and content boxes, and [`Fragment::anchor`] for the ink.
///
/// The padding and content boxes are derived from the border box and the edges. Three values
/// that nearly every fragment holds at a default are stated in two bits of the record, and the
/// store holds them for the other fragments: see [`FragmentExtra`] and
/// [`LayoutStore::fragment_extra`](crate::LayoutStore::fragment_extra).
#[derive(Clone, Debug)]
pub struct Fragment {
    /// This fragment's own name.
    pub key: FragKey,
    /// The box it is a piece of.
    pub box_: BoxKey,
    /// The element that box came from, or nothing for an anonymous box.
    pub node: Option<NodeKey>,
    /// The fragment this one is a piece inside, or nothing for the root.
    pub parent: Option<FragKey>,
    /// Border box in the stacking context's space, before this fragment's own transform.
    pub border_box: Rect<DevicePx, Device>,
    /// Union of everything this fragment *paints*, including shadow spread, outline offset and
    /// the bleed of any filter applied to it, in [`Fragment::anchor`]'s space.
    ///
    /// This is what damage is computed from, and under-reporting it leaves stale pixels behind. It
    /// is deliberately not what a fragment *reads*: a blurred or backdrop-filtered fragment samples
    /// pixels outside every rectangle it writes, and that extent is carried separately so that the
    /// many fragments which read nothing do not inflate damage.
    pub ink: Rect<DevicePx, Device>,
    /// The same union, in this fragment's own space rather than in the anchor's.
    ///
    /// The two differ by the matrix from [`Fragment::transform`] to [`Fragment::anchor`], and both
    /// are kept because they answer different questions. Damage is measured in real pixels and
    /// wants the anchored rectangle, which one matrix puts on the device. The hit index wants this
    /// one: an entry filed under the rectangle it occupies *on the device* stops being true the
    /// moment its coordinate system moves, and
    /// nothing walks a fragment whose matrix changed under it — see
    /// [`hit::HitEntry::envelope`](crate::fragment::hit::HitEntry::envelope).
    pub local_ink: Rect<DevicePx, Device>,
    /// Union of [`Fragment::ink`] over this fragment and every fragment below it, in this
    /// fragment's [`Fragment::anchor`].
    ///
    /// A fragment below that measures its ink in another anchor — the content space of a scroll
    /// container, see [`scroll_nodes`] — is put into this one before the union is taken.
    ///
    /// This is what lets a paint pass skip a whole clean subtree at once instead of descending it
    /// to find out that nothing in it intersects the damage.
    pub subtree_ink: Rect<DevicePx, Device>,
    /// The border widths, already resolved and snapped.
    pub border: Edges<DevicePx>,
    /// The padding widths, already resolved and snapped.
    pub padding: Edges<DevicePx>,
    /// The room the box's scrollbars take inside its padding, which the content box leaves out.
    ///
    /// Zero for every fragment except a scroll container with classic scrollbars.
    pub scrollbar: Size<DevicePx, Device>,
    /// The chain of clips this fragment is drawn under.
    pub clip: ClipId,
    /// The transform applied to it, if any.
    pub transform: Option<SpatialId>,
    /// The coordinate system [`Fragment::ink`] and [`Fragment::subtree_ink`] are measured in.
    ///
    /// Those two rectangles are device pixels only after this system's matrix is applied. It is
    /// the document's root space, which resolves to the identity unless the poison test mode is
    /// on — see [`zgui_scene::spatial::poison`] — or the content space of the nearest scroll
    /// container above that draws its content in one — see [`scroll_nodes`]. Use
    /// [`Fragment::on_device`] and the accessors beside it, never the raw rectangles, wherever
    /// device pixels are meant. Two fragments' raw rectangles compare only when they name the
    /// same anchor.
    pub anchor: SpatialId,
    /// The stacking context it belongs to, when [`Packed::STACKING`] is set.
    stacking: u32,
    /// The scroll frame it moves with, when [`Packed::SCROLL`] is set.
    scroll: u32,
    /// What it draws.
    pub kind: FragmentKind,
    /// What painting, damage and hit testing branch on.
    pub flags: FragmentFlags,
    /// Whether this fragment's subtree is pairwise non-overlapping.
    ///
    /// Decided over fragment ink and over the whole subtree, never over the primitives a
    /// particular frame emitted: a frame that painted half the subtree would otherwise decide
    /// differently from a frame that painted all of it, and the two would differ by a pixel.
    pub subtree_disjoint: bool,
    /// Whether every piece at and below this fragment moves by the same vector when the box above
    /// it moves.
    ///
    /// Three things break that promise and each is recorded here rather than looked for later. A
    /// sticky box's shift is measured against a scrollport it does not move with, so it stays put
    /// while its neighbours slide past. A box positioned against the viewport takes none of the
    /// scroll offsets above it, so it does not move at all. And a transformed box's matrix is
    /// composed against its own border box, so moving it issues a different matrix rather than the
    /// same one somewhere else.
    ///
    /// A clip is not on that list: a clipping box's rectangle moves with the box, so the chain its
    /// descendants are drawn under is the same chain measured somewhere else.
    pub subtree_rigid: bool,
    /// Which optional values the record holds, and how it states the [`FragmentExtra`] values.
    pub(crate) packed: Packed,
}

/// The values a fragment keeps outside its record.
///
/// Nearly every fragment holds the same values here: no fingerprint of content, the identity
/// matrix, and a clip chain measured in its own coordinate system or no clip chain. The record
/// states those cases in two bits. The store holds the values of each other fragment. Read them
/// with [`LayoutStore::fragment_extra`](crate::LayoutStore::fragment_extra).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FragmentExtra {
    /// The coordinate system this fragment's parent laid out in, set only when the fragment is
    /// clipped.
    ///
    /// It differs from [`Fragment::transform`] exactly by this fragment's own transform. It is the
    /// space of the clip chain's links only when no transformed box sits between this fragment's
    /// parent and the ancestors that clip it. Each link names its own space, and the hit index
    /// reads that. The fragment diff compares this field between frames.
    pub clip_transform: Option<SpatialId>,
    /// A fingerprint of the matrix that coordinate system resolved to when this was composed.
    ///
    /// Held because the name does not move when the matrix under it does: a box being moved
    /// rewrites the value in its coordinate system and keeps the name, so anything comparing this
    /// fragment against the one it replaces, or against what it drew last time, would otherwise see
    /// nothing at all change.
    pub transform_hash: u64,
    /// A fingerprint of what this fragment draws that its rectangles do not describe.
    ///
    /// Zero for nearly every fragment: what a box paints is decided from its style, and a style
    /// change damages the box directly. A *line* is the exception, because what it draws is decided
    /// by the inline formatting context around it — `text-overflow` cuts a line short and marks the
    /// cut, and both the cut and the mark can move while the line box stays exactly where it was.
    /// A comparison over rectangles alone would call that identical and never redraw it.
    pub content_hash: u64,
}

impl FragmentExtra {
    /// What a new fragment holds: no clip space, a zero matrix fingerprint and no content
    /// fingerprint.
    pub const NONE: Self = Self {
        clip_transform: None,
        transform_hash: 0,
        content_hash: 0,
    };
}

/// The fingerprint of the identity matrix, which nearly every fragment is composed under.
pub fn identity_hash() -> u64 {
    static HASH: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *HASH.get_or_init(|| {
        use zgui_scene::Content;
        zgui_geom::Matrix4::IDENTITY.content_hash()
    })
}

/// Which optional values a fragment record holds, and how it states its [`FragmentExtra`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Packed(u8);

impl Packed {
    /// [`Fragment::stacking`] holds a context.
    const STACKING: u8 = 1 << 0;
    /// [`Fragment::scroll`] holds a frame.
    const SCROLL: u8 = 1 << 1;
    /// [`FragmentExtra::clip_transform`] is [`Fragment::transform`]. Clear, it is nothing.
    const CLIP_IN_TRANSFORM: u8 = 1 << 2;
    /// [`FragmentExtra::transform_hash`] is [`identity_hash`]. Clear, it is zero.
    const IDENTITY: u8 = 1 << 3;
    /// The store holds this fragment's [`FragmentExtra`]. The two bits above are then clear.
    pub(crate) const SPILLED: u8 = 1 << 4;
    /// The bits that state the [`FragmentExtra`].
    const EXTRA: u8 = Self::CLIP_IN_TRANSFORM | Self::IDENTITY | Self::SPILLED;

    /// Whether every bit in `bits` is set.
    pub(crate) const fn has(self, bits: u8) -> bool {
        self.0 & bits == bits
    }

    /// The same bits with `bits` set or cleared.
    const fn with(self, bits: u8, on: bool) -> Self {
        if on {
            Self(self.0 | bits)
        } else {
            Self(self.0 & !bits)
        }
    }
}

impl Fragment {
    /// An empty fragment for `box_`, at the origin, clipping nothing and transforming nothing.
    pub fn new(key: FragKey, box_: BoxKey, kind: FragmentKind) -> Self {
        Self {
            key,
            box_,
            node: None,
            parent: None,
            border_box: Rect::ZERO,
            ink: Rect::ZERO,
            local_ink: Rect::ZERO,
            subtree_ink: Rect::ZERO,
            border: Edges::ZERO,
            padding: Edges::ZERO,
            scrollbar: Size::new(DevicePx(0.0), DevicePx(0.0)),
            clip: ClipId::ROOT,
            transform: None,
            anchor: SpatialId::VIEWPORT,
            stacking: 0,
            scroll: 0,
            kind,
            flags: FragmentFlags::EMPTY,
            subtree_disjoint: true,
            subtree_rigid: true,
            packed: Packed::default(),
        }
    }

    /// Padding box, on the same terms as [`Fragment::border_box`]: the border box less the
    /// border widths.
    #[inline]
    pub fn padding_box(&self) -> Rect<DevicePx, Device> {
        build::padding_box_of(self.border_box, self.border)
    }

    /// Content box, on the same terms: the padding box less the padding widths and the
    /// scrollbars.
    #[inline]
    pub fn content_box(&self) -> Rect<DevicePx, Device> {
        build::content_box_of(self.padding_box(), self.padding, self.scrollbar)
    }

    /// The stacking context it belongs to, if it establishes or joins one.
    #[inline]
    pub fn stacking(&self) -> Option<StackingContextId> {
        self.packed
            .has(Packed::STACKING)
            .then_some(StackingContextId(self.stacking))
    }

    /// Sets the stacking context it belongs to.
    #[inline]
    pub fn set_stacking(&mut self, stacking: Option<StackingContextId>) {
        self.stacking = stacking.map_or(0, |id| id.0);
        self.packed = self.packed.with(Packed::STACKING, stacking.is_some());
    }

    /// The scroll frame it moves with, if any.
    #[inline]
    pub fn scroll(&self) -> Option<ScrollFrameId> {
        self.packed
            .has(Packed::SCROLL)
            .then_some(ScrollFrameId(self.scroll))
    }

    /// Sets the scroll frame it moves with.
    #[inline]
    pub fn set_scroll(&mut self, scroll: Option<ScrollFrameId>) {
        self.scroll = scroll.map_or(0, |id| id.0);
        self.packed = self.packed.with(Packed::SCROLL, scroll.is_some());
    }

    /// The [`FragmentExtra`] the record states by itself, or nothing when the store holds it.
    #[inline]
    pub(crate) fn inline_extra(&self) -> Option<FragmentExtra> {
        if self.packed.has(Packed::SPILLED) {
            return None;
        }
        Some(FragmentExtra {
            clip_transform: if self.packed.has(Packed::CLIP_IN_TRANSFORM) {
                self.transform
            } else {
                None
            },
            transform_hash: if self.packed.has(Packed::IDENTITY) {
                identity_hash()
            } else {
                0
            },
            content_hash: 0,
        })
    }

    /// States `extra` in the record when the two bits can, and answers whether they can.
    ///
    /// Reads [`Fragment::transform`], so a caller writes that first. When this answers `false`,
    /// the record is marked as spilled, and the caller puts `extra` in the store.
    pub(crate) fn pack_extra(&mut self, extra: FragmentExtra) -> bool {
        let clip_in_transform = match extra.clip_transform {
            None => Some(false),
            Some(space) if self.transform == Some(space) => Some(true),
            Some(_) => None,
        };
        let identity = if extra.transform_hash == 0 {
            Some(false)
        } else if extra.transform_hash == identity_hash() {
            Some(true)
        } else {
            None
        };
        let packed = self.packed.with(Packed::EXTRA, false);
        match (clip_in_transform, identity, extra.content_hash) {
            (Some(clip_in_transform), Some(identity), 0) => {
                self.packed = packed
                    .with(Packed::CLIP_IN_TRANSFORM, clip_in_transform)
                    .with(Packed::IDENTITY, identity);
                true
            }
            _ => {
                self.packed = packed.with(Packed::SPILLED, true);
                false
            }
        }
    }

    /// A rectangle measured in [`Fragment::anchor`], put on the device through `placements`.
    ///
    /// This is the step from [`Fragment::ink`], [`Fragment::subtree_ink`] and any rectangle
    /// measured beside them to device pixels. The viewport anchor costs one comparison. Another
    /// anchor that resolves to the identity costs one slot read and no arithmetic. An anchor
    /// `placements` does not resolve leaves the rectangle as it is.
    #[inline]
    pub fn on_device(
        &self,
        rect: Rect<DevicePx, Device>,
        placements: &zgui_scene::Placements,
    ) -> Rect<DevicePx, Device> {
        if self.anchor == SpatialId::VIEWPORT {
            return rect;
        }
        transform::placed::onto_device(rect, Some(self.anchor), placements)
    }

    /// The same step through `tree` itself, for a caller between two publications of the
    /// placements.
    pub fn on_device_in(
        &self,
        rect: Rect<DevicePx, Device>,
        tree: &zgui_scene::SpatialTree,
    ) -> Rect<DevicePx, Device> {
        if self.anchor == SpatialId::VIEWPORT {
            return rect;
        }
        match tree.resolve(self.anchor) {
            Some(matrix) if matrix != zgui_geom::Matrix4::IDENTITY => {
                transform::transformed_bounds(&matrix, rect)
            }
            _ => rect,
        }
    }

    /// [`Fragment::ink`] in device pixels.
    pub fn device_ink(&self, placements: &zgui_scene::Placements) -> Rect<DevicePx, Device> {
        self.on_device(self.ink, placements)
    }

    /// [`Fragment::subtree_ink`] in device pixels.
    pub fn device_subtree_ink(
        &self,
        placements: &zgui_scene::Placements,
    ) -> Rect<DevicePx, Device> {
        self.on_device(self.subtree_ink, placements)
    }
}

#[cfg(test)]
mod tests {
    use super::FragmentFlags;

    #[test]
    fn ink_is_put_on_the_device_through_the_anchor() {
        use zgui_geom::{DevicePx, Matrix4, Point, Rect, Size};
        use zgui_scene::{OwnSpace, Placements, PropertyOwner, SpatialTree};

        let mut tree = SpatialTree::with_viewport();
        let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
        let moved = tree.space_of(
            tree.viewport(),
            owner,
            OwnSpace::of(Some(Matrix4::translation(5.0, 7.0, 0.0)), None, false),
        );
        let generation = zgui_arena::Generation::FIRST;
        let domain = zgui_arena::DomainId::FIRST;
        let mut fragment = super::Fragment::new(
            zgui_arena::Key::new(0, generation, domain),
            zgui_arena::Key::new(1, generation, domain),
            super::FragmentKind::Box,
        );
        let ink = Rect::new(
            Point::new(DevicePx(10.0), DevicePx(20.0)),
            Size::new(DevicePx(30.0), DevicePx(40.0)),
        );
        fragment.ink = ink;
        fragment.subtree_ink = ink;
        let placements = Placements::of(&tree);
        assert_eq!(
            fragment.device_ink(&placements),
            ink,
            "the viewport moves nothing"
        );

        fragment.anchor = moved;
        let shifted = ink.translate(Size::new(DevicePx(5.0), DevicePx(7.0)));
        assert_eq!(fragment.device_ink(&placements), shifted);
        assert_eq!(fragment.device_subtree_ink(&placements), shifted);
        assert_eq!(fragment.on_device_in(ink, &tree), shifted);
        assert_eq!(
            fragment.device_ink(&Placements::EMPTY),
            ink,
            "an anchor the placements do not resolve leaves the rectangle as it is"
        );
    }

    #[test]
    fn a_fragment_stays_within_three_cache_lines() {
        let size = size_of::<super::Fragment>();
        assert!(size <= 192, "Fragment grew to {size} bytes");
    }

    /// A fragment named and composed nowhere, for tests of its own record.
    fn fragment() -> super::Fragment {
        let generation = zgui_arena::Generation::FIRST;
        let domain = zgui_arena::DomainId::FIRST;
        super::Fragment::new(
            zgui_arena::Key::new(0, generation, domain),
            zgui_arena::Key::new(1, generation, domain),
            super::FragmentKind::Box,
        )
    }

    #[test]
    fn the_padding_and_content_boxes_are_derived_as_the_composing_walk_places_them() {
        use zgui_geom::{DevicePx, Edges, Point, Rect, Size};

        let mut fragment = fragment();
        fragment.border_box = Rect::new(
            Point::new(DevicePx(10.5), DevicePx(20.25)),
            Size::new(DevicePx(100.0), DevicePx(60.0)),
        );
        fragment.border = Edges::uniform(DevicePx(2.0));
        fragment.padding = Edges {
            top: DevicePx(3.0),
            right: DevicePx(4.0),
            bottom: DevicePx(5.0),
            left: DevicePx(6.0),
        };
        fragment.scrollbar = Size::new(DevicePx(12.0), DevicePx(0.0));
        let padding_box = fragment.border_box.inset(fragment.border);
        assert_eq!(fragment.padding_box(), padding_box);
        let inner = padding_box.inset(fragment.padding);
        assert_eq!(fragment.content_box().origin, inner.origin);
        assert_eq!(
            fragment.content_box().size,
            Size::new(DevicePx(inner.size.width.0 - 12.0), inner.size.height)
        );

        fragment.scrollbar = Size::new(DevicePx(500.0), DevicePx(0.0));
        assert_eq!(
            fragment.content_box().size.width.0,
            0.0,
            "a gutter wider than the box leaves no content box"
        );
    }

    #[test]
    fn the_stacking_context_and_the_scroll_frame_round_trip_every_value() {
        use zgui_scene::{ScrollFrameId, StackingContextId};

        let mut fragment = fragment();
        assert_eq!(fragment.stacking(), None);
        assert_eq!(fragment.scroll(), None);
        for value in [0, 1, 77, u32::MAX] {
            fragment.set_stacking(Some(StackingContextId(value)));
            fragment.set_scroll(Some(ScrollFrameId(value)));
            assert_eq!(fragment.stacking(), Some(StackingContextId(value)));
            assert_eq!(fragment.scroll(), Some(ScrollFrameId(value)));
        }
        fragment.set_stacking(None);
        assert_eq!(fragment.stacking(), None);
        assert_eq!(fragment.scroll(), Some(ScrollFrameId::VIEWPORT));
        fragment.set_scroll(None);
        assert_eq!(fragment.scroll(), None);
    }

    #[test]
    fn the_record_states_the_common_extras_and_spills_the_rest() {
        use super::{FragmentExtra, identity_hash};

        let mut fragment = fragment();
        assert_eq!(
            fragment.inline_extra(),
            Some(FragmentExtra::NONE),
            "a new fragment holds the defaults it always held"
        );
        let mut tree = zgui_scene::SpatialTree::with_viewport();
        let space = tree.viewport();
        let other = tree.space_of(
            space,
            zgui_scene::PropertyOwner::new(2).expect("a handle is never the empty word"),
            zgui_scene::OwnSpace::of(
                Some(zgui_geom::Matrix4::translation(1.0, 0.0, 0.0)),
                None,
                false,
            ),
        );
        assert_ne!(space, other);
        fragment.transform = Some(space);
        let common = FragmentExtra {
            clip_transform: Some(space),
            transform_hash: identity_hash(),
            content_hash: 0,
        };
        assert!(fragment.pack_extra(common));
        assert_eq!(fragment.inline_extra(), Some(common));
        assert!(fragment.pack_extra(FragmentExtra::NONE));
        assert_eq!(fragment.inline_extra(), Some(FragmentExtra::NONE));

        for rare in [
            FragmentExtra {
                content_hash: 5,
                ..common
            },
            FragmentExtra {
                transform_hash: 9,
                ..common
            },
            FragmentExtra {
                clip_transform: Some(other),
                ..common
            },
        ] {
            assert!(!fragment.pack_extra(rare), "{rare:?} needs the store");
            assert_eq!(fragment.inline_extra(), None);
        }
        assert!(fragment.pack_extra(common), "a spilled fragment comes back");
        assert_eq!(fragment.inline_extra(), Some(common));
    }

    #[test]
    fn flags_union_and_remove_independently() {
        let flags = FragmentFlags::EMPTY
            .union(FragmentFlags::CLIPS_CHILDREN)
            .union(FragmentFlags::IS_STICKY);
        assert!(flags.contains(FragmentFlags::CLIPS_CHILDREN));
        assert!(flags.contains(FragmentFlags::IS_STICKY));
        assert!(!flags.contains(FragmentFlags::HAS_TRANSFORM));
        let fewer = flags.without(FragmentFlags::IS_STICKY);
        assert!(fewer.contains(FragmentFlags::CLIPS_CHILDREN));
        assert!(!fewer.contains(FragmentFlags::IS_STICKY));
    }

    #[test]
    fn the_empty_set_contains_only_itself() {
        assert!(FragmentFlags::EMPTY.contains(FragmentFlags::EMPTY));
        assert!(!FragmentFlags::EMPTY.contains(FragmentFlags::CLIPS_CHILDREN));
        assert!(FragmentFlags::CLIPS_CHILDREN.contains(FragmentFlags::EMPTY));
    }
}
