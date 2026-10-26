//! The interned paint table.

use zgui_color::Color;

use crate::content::{Content, ContentHash};
use crate::id::PaintId;
use crate::paint::reference::{PaintKind, PaintRef};
use crate::paint::source::Paint;
use crate::spatial::PropertyOwner;
use crate::table::{ChangeCoverage, Table, TableVersion};

/// Which of a box's paints an owned entry holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaintRole {
    /// The background colour.
    Background,
    /// The border stroke, one colour or one per side.
    BorderSides,
    /// The outline stroke.
    Outline,
}

/// The name of an owned paint entry: the box it belongs to and which of its paints it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PaintOwner {
    /// The box.
    pub owner: PropertyOwner,
    /// Which of its paints.
    pub role: PaintRole,
}

impl PaintOwner {
    /// The name of `role` of the box `owner` names.
    pub const fn new(owner: PropertyOwner, role: PaintRole) -> Self {
        Self { owner, role }
    }
}

/// One paint table entry: a paint source, and the name of the box paint it is when it has one.
///
/// An unnamed entry is its paint: two shapes that computed to the same paint share it. A named
/// entry is its name: the same paint of the same box is the same entry, whatever colour it holds.
/// An animation that moves a colour therefore rewrites the slot the box already draws through, and
/// every record naming the slot keeps replaying. This is the mechanism of
/// [`ClipTable::push_named`](crate::ClipTable::push_named).
///
/// Named and unnamed entries never compare equal, whatever their paint.
#[derive(Clone, Debug)]
pub struct PaintEntry {
    /// The paint source.
    pub paint: Paint,
    /// The box paint the entry is, where it is owned.
    pub name: Option<PaintOwner>,
}

impl PartialEq for PaintEntry {
    fn eq(&self, other: &Self) -> bool {
        match (self.name, other.name) {
            (None, None) => self.paint == other.paint,
            (name, other_name) => name == other_name,
        }
    }
}

impl Content for PaintEntry {
    fn content_hash(&self) -> u64 {
        match self.name {
            None => self.paint.content_hash(),
            Some(name) => ContentHash::new()
                .u32(u32::MAX)
                .u64(name.owner.get())
                .u32(name.role as u32)
                .finish(),
        }
    }

    fn same_stored_value(&self, other: &Self) -> bool {
        self.name == other.name && self.paint == other.paint
    }
}

/// Every paint source in the document, interned by content and kept across frames.
///
/// Entries are interned by content, except for the owned entries [`PaintTable::owned`] makes,
/// which are interned by name. See [`PaintEntry`].
#[derive(Clone, Debug, Default)]
pub struct PaintTable {
    /// The entries.
    table: Table<PaintId, PaintEntry>,
}

impl PaintTable {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// The id of a flat colour.
    pub fn solid(&mut self, color: Color) -> PaintId {
        self.intern(Paint::Solid(color))
    }

    /// The id for `paint`, interning it if this is the first time it has been seen.
    pub fn intern(&mut self, paint: Paint) -> PaintId {
        self.table.intern(PaintEntry { paint, name: None })
    }

    /// The reference a primitive stores for `id`, with the family read off the entry.
    ///
    /// Prefer this to building a [`PaintRef`] by hand: the family and the index are two halves of
    /// one fact, and a shader told "solid" about a gradient reads the wrong storage.
    pub fn reference(&self, id: PaintId) -> PaintRef {
        let kind = match self.get(id) {
            Some(Paint::Solid(_)) => PaintKind::Solid,
            Some(Paint::Gradient { .. }) => PaintKind::Gradient,
            Some(Paint::Image { .. }) => PaintKind::Image,
            Some(Paint::Sides(_)) => PaintKind::Sides,
            None => return PaintRef::NONE,
        };
        PaintRef::new(kind, id)
    }

    /// Interns `paint` and returns the reference a primitive stores for it, in one step.
    pub fn add(&mut self, paint: Paint) -> PaintRef {
        let id = self.intern(paint);
        self.reference(id)
    }

    /// Puts `paint` in the entry `name` owns, and returns the reference a primitive stores for it.
    ///
    /// The entry keeps its slot while its paint changes, so the change is one slot rewritten in
    /// place and one journal entry for the table's readers.
    ///
    /// Use an owned slot only for box decorations. A vector item keys its cached encoding on
    /// [`PaintTable::content_hash`], which does not change when an owned paint is rewritten.
    pub fn owned(&mut self, name: PaintOwner, paint: Paint) -> PaintRef {
        let id = self.table.intern(PaintEntry {
            paint,
            name: Some(name),
        });
        self.reference(id)
    }

    /// The slot `name` owns, if it has one.
    pub fn owned_slot(&self, name: PaintOwner) -> Option<PaintId> {
        self.table.find(&PaintEntry {
            paint: Paint::Solid(Color::TRANSPARENT),
            name: Some(name),
        })
    }

    /// Rewrites the slot `name` owns to `paint`, and reports what that did.
    ///
    /// A primitive carries the family of its paint beside the slot, so a paint of another family
    /// is refused: the primitives naming the slot must be encoded again for it.
    pub fn rewrite(&mut self, name: PaintOwner, paint: Paint) -> PaintRewrite {
        let Some(id) = self.owned_slot(name) else {
            return PaintRewrite::Absent;
        };
        if self.get(id).map(Paint::kind_tag) != Some(paint.kind_tag()) {
            return PaintRewrite::OtherKind;
        }
        self.table.intern(PaintEntry {
            paint,
            name: Some(name),
        });
        PaintRewrite::Written
    }

    /// The paint `id` resolves to, without marking it as used.
    pub fn get(&self, id: PaintId) -> Option<&Paint> {
        self.table.get(id).map(|entry| &entry.paint)
    }

    /// The paint `id` resolves to, marking it as used this frame.
    pub fn use_of(&mut self, id: PaintId) -> Option<&Paint> {
        self.table.use_of(id).map(|entry| &entry.paint)
    }

    /// See [`Table::version`].
    pub fn version(&self) -> TableVersion {
        self.table.version()
    }

    /// See [`Table::changes_since`].
    pub fn changes_since(
        &self,
        version: TableVersion,
        changed: &mut Vec<PaintId>,
    ) -> ChangeCoverage {
        self.table.changes_since(version, changed)
    }

    /// See [`Table::begin_frame`].
    pub fn begin_frame(&mut self) {
        self.table.begin_frame();
    }

    /// See [`Table::generation`].
    pub fn generation(&self) -> u64 {
        self.table.generation()
    }

    /// See [`Table::len`].
    pub fn len(&self) -> usize {
        self.table.len()
    }

    /// See [`Table::slots`].
    pub fn slots(&self) -> usize {
        self.table.slots()
    }

    /// See [`Table::is_empty`].
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// See [`Table::content_hash`].
    ///
    /// An owned entry hashes its name, so the hash holds while its paint is rewritten.
    pub fn content_hash(&self, id: PaintId) -> Option<u64> {
        self.table.content_hash(id)
    }

    /// See [`Table::contains`].
    pub fn contains(&self, id: PaintId) -> bool {
        self.table.contains(id)
    }

    /// See [`Table::retain`].
    pub fn retain(&mut self, id: PaintId) -> bool {
        self.table.retain(id)
    }

    /// See [`Table::release`].
    pub fn release(&mut self, id: PaintId) -> bool {
        self.table.release(id)
    }

    /// See [`Table::refs`].
    pub fn refs(&self, id: PaintId) -> Option<u32> {
        self.table.refs(id)
    }

    /// See [`Table::evict_least_recently_used`].
    ///
    /// An owned entry goes only when nothing holds it, as every other entry does. No runtime sweep
    /// calls this yet: the unit tests pin what a future paint sweep keeps.
    pub fn evict_least_recently_used(&mut self) -> usize {
        self.table.evict_least_recently_used()
    }

    /// See [`Table::evict_unreachable`]. No paint resolves through another.
    ///
    /// A held entry is reachable, so an owned entry a record draws through stays while the record
    /// stands. No runtime sweep calls this yet: the unit tests pin what a future paint sweep keeps.
    pub fn evict_unreachable(&mut self, keep_generations: u64, cap: usize) -> usize {
        self.table
            .evict_unreachable(keep_generations, cap, |_| None)
    }
}

/// What [`PaintTable::rewrite`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintRewrite {
    /// The slot holds the new paint.
    Written,
    /// The name owns no slot.
    Absent,
    /// The slot holds a paint of another family, and was left as it was.
    OtherKind,
}
