//! The fourth table: text brushes, which are mutated rather than interned.

use rustc_hash::FxHashMap;
use zgui_color::Color;

use crate::id::PaintSlot;

/// The colour a run of glyphs is drawn in.
///
/// Premultiplied, gamma-encoded sRGB — the one encoding everything in the pipeline composites in —
/// so that reaching a draw call from here is a copy and never a conversion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextPaint {
    /// Premultiplied, gamma-encoded sRGB, in red, green, blue, alpha order.
    pub color: [f32; 4],
}

impl TextPaint {
    /// The brush for `color`.
    pub fn new(color: Color) -> Self {
        Self {
            color: color.to_premultiplied_srgb(),
        }
    }
}

/// Text brushes, addressed by [`PaintSlot`] and mutable in place.
///
/// A shaped paragraph is expensive and is cached; it stores a [`PaintSlot`] rather than a colour.
/// Switching theme then re-colours every cached paragraph by writing through the slots, with no
/// paragraph re-shaped and no cache invalidated.
///
/// That is exactly why entries here are **not** interned by content. A slot is claimed against a
/// caller-supplied key — the identity of the cascade result a paragraph inherited its colour from —
/// so two paragraphs share a slot precisely when they share a cascade result, which is the set a
/// theme change re-resolves together. Interning by resolved colour would put a paragraph whose
/// colour came from a theme variable in the same slot as one whose colour was written literally,
/// and re-colouring the first would silently re-colour the second.
///
/// Entries live as long as the document. There is no eviction and no reference counting, because a
/// slot that disappeared while a cached paragraph still named it would draw that paragraph in
/// whatever colour landed in the slot next.
///
/// ```
/// use zgui_color::Color;
/// use zgui_scene::{TextPaint, TextPaintTable};
///
/// let mut brushes = TextPaintTable::new();
/// // The key is the identity of the inherited text style, not the colour itself.
/// let slot = brushes.slot_for(0xcafe, || TextPaint::new(Color::srgb(0.0, 0.0, 0.0, 1.0)));
/// assert_eq!(brushes.slot_for(0xcafe, || unreachable!()), slot);
///
/// // A theme change is a write through the slot: no paragraph is re-shaped.
/// brushes.set(slot, TextPaint::new(Color::srgb(1.0, 1.0, 1.0, 1.0)));
/// assert_eq!(brushes.get(slot).unwrap().color, [1.0, 1.0, 1.0, 1.0]);
/// ```
#[derive(Clone, Debug, Default)]
pub struct TextPaintTable {
    /// The brushes, indexed by slot.
    slots: Vec<TextPaint>,
    /// Caller key to slot.
    by_key: FxHashMap<u64, PaintSlot>,
}

impl TextPaintTable {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many slots exist.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether no slot has been claimed.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The slot `key` owns, claiming one with `build` if it does not have one yet.
    ///
    /// `key` identifies the *cascade result* a run inherited its colour from, not the colour. Two
    /// runs sharing a cascade result share a slot; two runs that merely computed to the same colour
    /// do not.
    pub fn slot_for(&mut self, key: u64, build: impl FnOnce() -> TextPaint) -> PaintSlot {
        if let Some(slot) = self.by_key.get(&key) {
            return *slot;
        }
        let slot = PaintSlot(self.slots.len() as u32);
        self.slots.push(build());
        self.by_key.insert(key, slot);
        slot
    }

    /// The slot `key` already resolves to, if it has one.
    ///
    /// A slot belongs to the cascade result it was claimed against rather than to any one element,
    /// so this is what a caller asks before deciding that an element's slot is its own to rewrite
    /// or to re-point: a result that already has a slot has it because something is already drawn
    /// through it, and the answer is not the asking element's to change.
    pub fn slot_of(&self, key: u64) -> Option<PaintSlot> {
        self.by_key.get(&key).copied()
    }

    /// Points `key` at a slot that already exists.
    ///
    /// A cascade result is a new object every time the cascade runs, so an element whose colour
    /// changed arrives with a key nothing has seen before while everything already drawn still
    /// names the slot the *old* key claimed. Aliasing is how the two are reconciled: the new key is
    /// pointed at the existing slot, the slot is rewritten, and nothing has to be re-shaped.
    ///
    /// ```
    /// use zgui_color::Color;
    /// use zgui_scene::{TextPaint, TextPaintTable};
    ///
    /// let mut brushes = TextPaintTable::new();
    /// let slot = brushes.slot_for(1, || TextPaint::new(Color::srgb(0.0, 0.0, 0.0, 1.0)));
    /// brushes.alias(2, slot);
    /// assert_eq!(brushes.slot_for(2, || unreachable!()), slot);
    /// ```
    pub fn alias(&mut self, key: u64, slot: PaintSlot) {
        if (slot.0 as usize) < self.slots.len() {
            self.by_key.insert(key, slot);
        }
    }

    /// Stops `key` resolving to a slot, leaving the slot itself and everything drawn through it.
    ///
    /// A cascade result is a new object every time the cascade runs, so an element that is
    /// animating a colour supersedes its own key on every frame. Without this the table keeps every
    /// key it was ever handed — one per frame for as long as the animation runs — and each of those
    /// is a number whose allocation the caller has stopped holding, so the address may be handed to
    /// a *different* cascade result later and that result would silently inherit this slot's
    /// colour.
    ///
    /// The slot is deliberately left in place: what is already shaped names slots, not keys, and a
    /// slot removed under a cached paragraph would draw it in whatever landed there next.
    ///
    /// ```
    /// use zgui_color::Color;
    /// use zgui_scene::{TextPaint, TextPaintTable};
    ///
    /// let mut brushes = TextPaintTable::new();
    /// let slot = brushes.slot_for(1, || TextPaint::new(Color::srgb(0.0, 0.0, 0.0, 1.0)));
    /// brushes.forget(1);
    /// assert_eq!(brushes.keys(), 0, "the key is gone");
    /// assert!(brushes.get(slot).is_some(), "what is already drawn through it is not");
    /// ```
    pub fn forget(&mut self, key: u64) {
        self.by_key.remove(&key);
    }

    /// How many keys resolve to a slot.
    ///
    /// Slots are permanent and keys are not, so this is the number that says whether the table is
    /// keeping up with a document whose cascade results are being replaced every frame.
    pub fn keys(&self) -> usize {
        self.by_key.len()
    }

    /// The brush in `slot`.
    pub fn get(&self, slot: PaintSlot) -> Option<&TextPaint> {
        self.slots.get(slot.0 as usize)
    }

    /// Replaces the brush in `slot`, re-colouring everything that refers to it.
    ///
    /// Silently does nothing for a slot that was never claimed, which cannot happen for a slot this
    /// table handed out.
    pub fn set(&mut self, slot: PaintSlot, paint: TextPaint) {
        if let Some(held) = self.slots.get_mut(slot.0 as usize) {
            *held = paint;
        }
    }

    /// Rewrites every brush, which is what a theme change does.
    pub fn recolour(&mut self, mut colour: impl FnMut(PaintSlot, TextPaint) -> TextPaint) {
        for (index, paint) in self.slots.iter_mut().enumerate() {
            *paint = colour(PaintSlot(index as u32), *paint);
        }
    }
}
