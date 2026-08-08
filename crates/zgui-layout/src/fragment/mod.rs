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
pub mod diff;
pub mod filter;
pub mod hit;
pub mod index;
pub mod ink;
pub mod stacking;
pub mod sticky;
pub mod transform;

use smallvec::SmallVec;
use zgui_arena::Key;
use zgui_dom::NodeKey;
use zgui_dom::host::ReplacedId;
use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx, Edges, Rect};
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
pub struct FragmentFlags(u8);

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

    /// Whether every flag in `other` is set here.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// The union of two sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The same set with `other` removed.
    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// The raw bits, for transcripts.
    pub const fn bits(self) -> u8 {
        self.0
    }
}

/// One painted piece of one box, in absolute device pixels.
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
    /// Padding box, on the same terms.
    pub padding_box: Rect<DevicePx, Device>,
    /// Content box, on the same terms.
    pub content_box: Rect<DevicePx, Device>,
    /// Union of everything this fragment *paints*, including shadow spread, outline offset and
    /// the bleed of any filter applied to it.
    ///
    /// This is what damage is computed from, and under-reporting it leaves stale pixels behind. It
    /// is deliberately not what a fragment *reads*: a blurred or backdrop-filtered fragment samples
    /// pixels outside every rectangle it writes, and that extent is carried separately so that the
    /// many fragments which read nothing do not inflate damage.
    pub ink: Rect<DevicePx, Device>,
    /// The same union, in this fragment's own space rather than on the device.
    ///
    /// The two differ by exactly the matrix [`Fragment::transform`] names, and both are kept
    /// because they answer different questions. Damage is measured in real pixels and wants the
    /// device rectangle. The hit index wants this one: an entry filed under the rectangle it
    /// occupies *on the device* stops being true the moment its coordinate system moves, and
    /// nothing walks a fragment whose matrix changed under it — see
    /// [`hit::HitEntry::envelope`](crate::fragment::hit::HitEntry::envelope).
    pub local_ink: Rect<DevicePx, Device>,
    /// Union of [`Fragment::ink`] over this fragment and every fragment below it.
    ///
    /// This is what lets a paint pass skip a whole clean subtree at once instead of descending it
    /// to find out that nothing in it intersects the damage.
    pub subtree_ink: Rect<DevicePx, Device>,
    /// The border widths, already resolved and snapped.
    pub border: Edges<DevicePx>,
    /// The padding widths, already resolved and snapped.
    pub padding: Edges<DevicePx>,
    /// The chain of clips this fragment is drawn under.
    pub clip: ClipId,
    /// The matrix the clip chain's own rectangles are measured in, if it is not the identity.
    ///
    /// A clip is recorded in the space of the box that imposed it, which is *not* the space of the
    /// content it clips as soon as that content carries a transform of its own: a box translated
    /// three hundred pixels sideways keeps its rectangle at the origin and is drawn elsewhere, so
    /// testing its ancestor's clip against a point in the box's own space would test the clip in
    /// the wrong place and let the box answer where nothing is drawn. This is the matrix that puts
    /// the clip's rectangles back where they were measured.
    ///
    /// It differs from [`Fragment::transform`] exactly by this fragment's own transform.
    ///
    /// One matrix describes the whole chain, which is exact unless a transformed box sits *between*
    /// two clipping ancestors — then the outer links were measured before that transform and this
    /// names only the inner ones' space. Such a chain is tested slightly too permissively, which
    /// costs an extra answer at the edge of the outer clip and never a missing one.
    pub clip_transform: Option<SpatialId>,
    /// The transform applied to it, if any.
    pub transform: Option<SpatialId>,
    /// A fingerprint of the matrix that coordinate system resolved to when this was composed.
    ///
    /// Held because the name does not move when the matrix under it does: a box being moved
    /// rewrites the value in its coordinate system and keeps the name, so anything comparing this
    /// fragment against the one it replaces, or against what it drew last time, would otherwise see
    /// nothing at all change.
    pub transform_hash: u64,
    /// The stacking context it belongs to, if it establishes or joins one.
    pub stacking: Option<StackingContextId>,
    /// The scroll frame it moves with, if any.
    pub scroll: Option<ScrollFrameId>,
    /// What it draws.
    pub kind: FragmentKind,
    /// A fingerprint of what this fragment draws that its rectangles do not describe.
    ///
    /// Zero for nearly every fragment: what a box paints is decided from its style, and a style
    /// change damages the box directly. A *line* is the exception, because what it draws is decided
    /// by the inline formatting context around it — `text-overflow` cuts a line short and marks the
    /// cut, and both the cut and the mark can move while the line box stays exactly where it was.
    /// A comparison over rectangles alone would call that identical and never redraw it.
    pub content_hash: u64,
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
            padding_box: Rect::ZERO,
            content_box: Rect::ZERO,
            ink: Rect::ZERO,
            local_ink: Rect::ZERO,
            subtree_ink: Rect::ZERO,
            border: Edges::ZERO,
            padding: Edges::ZERO,
            clip: ClipId::ROOT,
            clip_transform: None,
            transform: None,
            transform_hash: 0,
            stacking: None,
            scroll: None,
            kind,
            content_hash: 0,
            flags: FragmentFlags::EMPTY,
            subtree_disjoint: true,
            subtree_rigid: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FragmentFlags;

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
