//! The way back from an element whose colour moved to the slot its glyphs are drawn through.
//!
//! A shaped paragraph stores a brush slot rather than a colour, so a colour change is a write
//! through the slot and costs no shaping at all. What makes that harder than it sounds is that
//! neither end of it is stable. The element is: a `NodeKey` outlives every restyle. The *cascade
//! result* is not — it is a new object every time the cascade runs, and it is what the slot was
//! claimed against, so the key an update arrives with is a key nothing has ever seen while
//! everything already on the screen still names the slot the previous one claimed.
//!
//! So the element is what a slot is remembered per, and the cascade result it was last claimed
//! against is held here beside it: held rather than noted, because the key is an address, and an
//! address is an identity only while the allocation behind it is alive.

use rustc_hash::FxHashMap;
use zgui_dom::NodeKey;
use zgui_scene::{PaintSlot, TextPaint, TextPaintTable};
use zgui_style::{TextPaintUpdate, TextRun};
use zgui_text_style::TextPaintKey;

/// Which of an element's runs a remembered slot belongs to.
///
/// An element is the source of more than one run — its own content, and whatever its `::before` and
/// `::after` generate — and each is cascaded separately, so each holds its own colour and claims a
/// brush slot of its own. Remembering them under the element alone keeps one of them: the others
/// are then never rewritten, and a field's placeholder stays the colour it was shaped in while the
/// field around it follows the theme.
pub(crate) type SlotKey = (NodeKey, TextRun);

/// What this batch is doing to one slot.
///
/// A slot may only be rewritten where it stands if *every* element still drawn through it is being
/// re-coloured in this same batch — and if they all agree on what to. Both halves are needed, and
/// the second is the one that is easy to believe comes for free: a slot is claimed per cascade
/// result, so the elements sharing one look as though they must always compute the same colour.
///
/// They need not, because [`TextPaintTable::alias`] deliberately merges cascade results. An element
/// whose result is rebuilt without its colour moving — which is every element that declares an
/// inherited-text property and is restyled for any reason at all, a hovered row, a toggled class, a
/// transition on something else — is pointed at the slot it is already using rather than given one
/// of its own. After that the slot answers to two results, and the next batch in which those two
/// diverge writes the slot twice: the second write wins, everything shaped through the slot is
/// drawn in the second element's colour, and nothing re-shapes, because both elements were told
/// they were re-coloured in place. The wrong colour then stands until something unrelated re-shapes
/// the text — a change of device scale is exactly that, which is why moving the window to a monitor
/// at another scale factor appears to fix it, or to break it.
struct Covering {
    /// How many of the slot's elements this batch is re-colouring.
    count: usize,
    /// The colour the first of them named.
    paint: TextPaint,
    /// Whether every one of them named that same colour.
    agreed: bool,
}

/// Which slot each element's text is drawn through, with the counts a batch is decided by.
///
/// The counts are kept beside the records rather than derived from them per batch: a document
/// with twenty thousand strings restyles one of them on most frames, and rebuilding two maps
/// over every record to answer for one is what charges a whole document's worth of work to a
/// frame that changed one element.
#[derive(Default)]
pub(crate) struct TextSlots {
    /// The record per element run.
    slots: FxHashMap<SlotKey, TextSlot>,
    /// How many records are drawn through each slot.
    owners: FxHashMap<PaintSlot, usize>,
    /// How many records answer to each cascade result's address.
    holders: FxHashMap<u64, usize>,
    /// The document's dropped-node count at the last sweep.
    swept_at: u64,
    /// Slots an update left with no record drawn through them, for the next sweep to release.
    orphaned: Vec<PaintSlot>,
}

/// The fewest nodes the document drops between two sweeps of the records.
pub(crate) const SWEEP_AFTER_DROPPED: u64 = 64;

impl TextSlots {
    /// The record for one element run, if it has been reported.
    pub(crate) fn get(&self, key: &SlotKey) -> Option<&TextSlot> {
        self.slots.get(key)
    }

    /// Files `held` under `key`, returning what it replaced.
    fn insert(&mut self, key: SlotKey, held: TextSlot) -> Option<TextSlot> {
        *self.owners.entry(held.slot).or_default() += 1;
        *self.holders.entry(address(&held.key)).or_default() += 1;
        let previous = self.slots.insert(key, held);
        if let Some(previous) = &previous {
            self.release(previous);
        }
        previous
    }

    /// Takes the counts a record contributed back.
    fn release(&mut self, held: &TextSlot) {
        if let Some(count) = self.owners.get_mut(&held.slot) {
            *count -= 1;
            if *count == 0 {
                self.owners.remove(&held.slot);
            }
        }
        if let Some(count) = self.holders.get_mut(&address(&held.key)) {
            *count -= 1;
            if *count == 0 {
                self.holders.remove(&address(&held.key));
            }
        }
    }

    /// How many records are drawn through `slot`.
    fn owners_of(&self, slot: PaintSlot) -> usize {
        self.owners.get(&slot).copied().unwrap_or_default()
    }

    /// Whether any record still answers to the cascade result at `address`.
    fn held_by_anyone(&self, address: u64) -> bool {
        self.holders.contains_key(&address)
    }

    /// How many records there are.
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether a sweep has slots to release, or the document dropped enough nodes since the last
    /// sweep for a pass over the records to pay.
    ///
    /// `dropped` is the document's dropped-node count. A pass over the records examines every
    /// record, so it waits until the document dropped at least an eighth as many nodes as there
    /// are records. The records of nodes that are gone are thus at most that many, and a pass
    /// costs a few checks per dropped node.
    pub(crate) fn owes_sweep(&self, dropped: u64) -> bool {
        !self.orphaned.is_empty() || self.records_due(dropped)
    }

    /// Whether the document dropped enough nodes since the last pass over the records.
    pub(crate) fn records_due(&self, dropped: u64) -> bool {
        dropped.saturating_sub(self.swept_at)
            >= SWEEP_AFTER_DROPPED.max(self.slots.len() as u64 / 8)
    }

    /// Releases the slots no record is drawn through, and drops the records of the elements
    /// `is_live` rejects when [`TextSlots::owes_sweep`] says a pass over them pays.
    ///
    /// `dropped` is the document's dropped-node count. A cascade result no record answers to any
    /// more is forgotten by the table, as [`retire`] forgets one. A slot no record is drawn through
    /// any more is released, unless a key still resolves to it. The released slots are returned,
    /// so that the text engine can drop the paragraphs shaped through them.
    pub(crate) fn sweep(
        &mut self,
        table: &mut TextPaintTable,
        dropped: u64,
        is_live: impl Fn(NodeKey) -> bool,
    ) -> Vec<PaintSlot> {
        let mut emptied = core::mem::take(&mut self.orphaned);
        if !self.records_due(dropped) {
            return self.release_emptied(table, emptied);
        }
        self.swept_at = dropped;
        let gone: Vec<SlotKey> = self
            .slots
            .keys()
            .filter(|(node, _)| !is_live(*node))
            .copied()
            .collect();
        for key in gone {
            let Some(held) = self.slots.remove(&key) else {
                continue;
            };
            self.release(&held);
            let address = address(&held.key);
            if !self.held_by_anyone(address)
                && let Some(mapped) = table.slot_of(address)
            {
                table.forget(address);
                if self.owners_of(mapped) == 0 {
                    emptied.push(mapped);
                }
            }
            if self.owners_of(held.slot) == 0 {
                emptied.push(held.slot);
            }
        }
        self.release_emptied(table, emptied)
    }

    /// Releases each of `emptied` that no record is drawn through and no key resolves to, and
    /// returns the ones released.
    fn release_emptied(
        &self,
        table: &mut TextPaintTable,
        mut emptied: Vec<PaintSlot>,
    ) -> Vec<PaintSlot> {
        emptied.sort_unstable_by_key(|slot| slot.0);
        emptied.dedup();
        emptied.retain(|slot| self.owners_of(*slot) == 0 && table.release(*slot));
        emptied
    }
}

/// Which slot one element's text is drawn through.
///
/// The slot is the one the element's own cascade result answers to in the table. Holding that
/// together is what makes the counts above mean what the in-place rewrite reads them as: a record
/// stands for its result as well as for its element, so a slot every record on it agrees about is
/// a slot every result that resolves to it agrees about.
pub(crate) struct TextSlot {
    /// The slot itself, which is what everything already shaped names.
    pub(crate) slot: PaintSlot,
    /// The cascade result the slot currently answers to.
    ///
    /// Held so that the address the table has it filed under stays this element's own. Dropped
    /// without being retired from the table, the allocation is freed, the next style built lands on
    /// the same address, and the paragraphs of an unrelated element claim this element's colour.
    key: TextPaintKey,
}

/// Writes each moved colour through the slot the element's glyphs read, and names every element
/// that had to leave the slot it was sharing.
///
/// A slot is rewritten in place only when *every* element drawn through it moved in the same batch,
/// which is what a theme change and an inherited transition both are. An element that leaves a slot
/// others still name cannot be re-coloured that way — what its glyphs name was decided when they
/// were shaped — so it is given a slot of its own and the caller is told that the shaping behind
/// **that element** is now wrong. The names are the answer rather than a flag, because the shaping
/// that is now wrong is the shaping of those runs and of no others: told only that something split,
/// a caller can do nothing but throw away every paragraph in the window.
///
/// # An identity that moved without a colour moving
///
/// The elements arriving here are the ones whose *cascade result* is a different object, which is
/// not the same question as whether they are drawn in a different colour: an element that declares
/// an inherited-text property is rebuilt at a fresh address by every cascade it takes part in,
/// whatever the values are, and generated content is reported on the identity of its whole cascade
/// result. So the colour already in the slot is compared before anything is concluded from the
/// identity. An element that is already drawn in the colour it now computes to needs no rewrite,
/// no slot of its own and no reshaping — only the new identity pointed at the slot it is already
/// using, so that the next paragraph flattened under it does not claim a second one.
///
/// Doing that here rather than declining to report it keeps two things true that a caller cannot
/// restore afterwards. The table files a slot under the *identity*, so an identity nothing pointed
/// at a slot would claim a fresh slot the first time a paragraph under it was flattened, and that
/// slot is one nothing rewrites when the colour later moves. And the in-place rewrite above is
/// decided by counting the elements a slot is shared between: an element that kept its colour is
/// still an element that moved off its old cascade result, so leaving it out of the count would
/// make a theme flip in which any single element's colour is unchanged look like a document-wide
/// split.
///
/// # An element that arrives on somebody else's result
///
/// A slot belongs to the cascade result it was claimed against, and an element that cascades onto a
/// result another element already claimed against is drawn through that result's slot from its next
/// shaping on. So the element moves onto it here and is named as split: the glyphs it shaped under
/// the slot it leaves read a slot its result stopped answering to, and only re-shaping brings the
/// two ends back together.
///
/// Leaving the record on the old slot is what the in-place rewrite then reads as one element's own
/// slot. Re-colouring that element writes the slot where it stands, under a result that still
/// answers to it, and every run flattened under that result afterwards is shaped into the other
/// element's colour — for as long as the shaping survives.
///
/// What one batch of paint updates did to the slots.
pub(crate) struct Applied {
    /// The elements whose text has to be shaped again under a slot of their own.
    pub(crate) split: Vec<SlotKey>,
    /// Whether any update was drawn in a colour its slot did not hold.
    ///
    /// False for a batch in which every element moved to a new cascade result of the same
    /// colour — a font change, a theme flip that left the text colour alone — which then rewrites
    /// no brush and owes the display list no copy of the table.
    pub(crate) recoloured: bool,
}

pub(crate) fn apply(
    slots: &mut TextSlots,
    table: &mut TextPaintTable,
    updates: &[TextPaintUpdate],
) -> Applied {
    // Whether each update is drawn in a colour the slot it is already using does not hold. Taken
    // before anything is written, because the loop below rewrites slots and a comparison made
    // half way through it would be a comparison against this batch's own work.
    let recoloured: Vec<bool> = updates
        .iter()
        .map(|update| {
            let paint = TextPaint::new(update.paint.color);
            slots
                .get(&(update.node, update.run))
                .is_none_or(|held| table.get(held.slot) != Some(&paint))
        })
        .collect();

    // Only the elements whose colour actually moved are counted, which is what makes leaving a
    // slot alone safe: a slot is rewritten where it stands only if every element still drawn
    // through it is having its colour rewritten to the same thing in this same batch. Both halves
    // are recorded here — see [`Covering`] for why the second is not implied by the first.
    let mut covered: FxHashMap<PaintSlot, Covering> = FxHashMap::default();
    for (update, moved) in updates.iter().zip(&recoloured) {
        if let (true, Some(held)) = (*moved, slots.get(&(update.node, update.run))) {
            let paint = TextPaint::new(update.paint.color);
            covered
                .entry(held.slot)
                .and_modify(|covering| {
                    covering.count += 1;
                    covering.agreed &= covering.paint == paint;
                })
                .or_insert(Covering {
                    count: 1,
                    paint,
                    agreed: true,
                });
        }
    }
    // How many records each touched slot is drawn through *before* the batch. Read once, because
    // the loop below moves records off their slots as it goes: a count read as each update is
    // decided would fall to the batch's own size by its last update, and the slot would then be
    // rewritten in place under the elements that stayed on it.
    let owners_at_start: FxHashMap<PaintSlot, usize> = covered
        .keys()
        .map(|slot| (*slot, slots.owners_of(*slot)))
        .collect();

    let recoloured_any = recoloured.iter().any(|moved| *moved);
    let mut split = Vec::new();
    let mut emptied = Vec::new();
    for (update, moved) in updates.iter().zip(&recoloured) {
        let paint = TextPaint::new(update.paint.color);
        let key = update.paint.key.clone();
        let claimed = address(&key);
        let held = slots.get(&(update.node, update.run));
        // The slot the arriving cascade result already answers to, which is where every run
        // flattened under it afterwards is drawn. See [`establish`] for why the slot belongs to
        // the result rather than to whichever element arrives on it.
        let owned = table.slot_of(claimed);
        let slot = match (held, owned) {
            (Some(held), Some(owned)) if owned != held.slot => {
                // The element has cascaded onto a result something else already claimed a slot
                // against, which is what losing a colour declaration and falling back to an
                // inherited one is. The result owns that slot, so the element follows its result:
                // the record moves onto it, and the glyphs shaped under the slot it leaves are
                // reported as shaping to do again.
                //
                // A record left on the other slot is what the counting below reads. The slot then
                // looks like this element's own, re-colouring the element rewrites it where it
                // stands, and every run later flattened under the result that still answers to it
                // is shaped into this element's colour.
                split.push((update.node, update.run));
                owned
            }
            (Some(held), _) if !*moved => {
                // Nothing is drawn differently, so nothing is rewritten and nothing is re-shaped.
                let slot = held.slot;
                establish(table, claimed, slot);
                slot
            }
            (Some(held), _) if rewritable_in_place(&covered, &owners_at_start, held.slot) => {
                let slot = held.slot;
                table.set(slot, paint);
                // The new cascade result is pointed at the slot everything already shaped still
                // names, so that the next paragraph flattened under the new colour does not claim
                // a second one and draw the two halves of one element in two colours. The arm
                // above holds a result that already answers to a slot, so this one only ever
                // points a result that answers to none.
                establish(table, claimed, slot);
                slot
            }
            (other, _) => {
                // An element leaving a slot that other elements still use cannot be re-coloured by
                // writing through it, and what its glyphs name was baked in when they were shaped.
                // The shaping is what has to go.
                //
                // So does the shaping of an element that was restyled before and has no record
                // here: its earlier report was never applied, and whatever it shaped meanwhile
                // claimed a slot of its own that nothing rewrites. Reporting it as split is what
                // gets that shaping dropped instead of drawn in the old colour for ever.
                if other.is_some() || update.restyled {
                    split.push((update.node, update.run));
                }
                let slot = table.slot_for(claimed, || paint);
                table.set(slot, paint);
                slot
            }
        };
        let previous = slots.insert((update.node, update.run), TextSlot { slot, key });
        if let Some(left) = previous.as_ref().map(|previous| previous.slot)
            && left != slot
        {
            emptied.push(left);
        }
        if let Some(unmapped) = retire(table, slots, previous, claimed) {
            emptied.push(unmapped);
        }
    }
    // An element that left a slot, or a key that was forgotten, can leave a slot nothing is drawn
    // through and nothing resolves to. The paragraphs of an element that left a slot still name it
    // until they are shaped again later in the frame, so the slot is released at the frame's end.
    slots.orphaned.extend(emptied);
    Applied {
        split,
        recoloured: recoloured_any,
    }
}

/// Points a cascade result at `slot`, unless it already resolves to one of its own.
///
/// The refusal is the whole of it. A slot is claimed against a cascade result and then *belongs* to
/// it: everything that resolves through the result is drawn through that slot, and the elements
/// that hold the slot are what keep rewriting it as the theme moves. An element arriving on a
/// result that already has a slot is not entitled to point the result at the slot its own glyphs
/// happen to name.
///
/// Nothing downstream can notice when it does. The two slots hold the same colour at that instant —
/// this batch has just written it into both — both go on being rewritten by the elements that hold
/// them, and the damage is right. What is wrong is the answer the table gives *afterwards*, to
/// everything that resolves through that result later: a paragraph flattened for the first time, a
/// tooltip opened into a layer of its own, and every string in the window on the frame a change of
/// device scale re-shapes them all. Each is sent to the other element's slot and drawn in the other
/// element's colour — which, where the two are on opposite sides of a theme boundary, is the colour
/// of the theme the window is no longer in.
///
/// That is why the symptom looks like it belongs to the monitor rather than to the flip. The table
/// is wrong from the moment of the flip and nothing reads the wrong entry until something re-shapes:
/// dragging the window onto an output at another scale re-shapes every string in it at once.
///
/// [`apply`] moves such an element onto the slot its result owns and reports its shaping as work to
/// do again, so the refusal here stands for the case it cannot reach: a result that answers to the
/// very slot it is being pointed at.
fn establish(table: &mut TextPaintTable, claimed: u64, slot: PaintSlot) {
    if table.slot_of(claimed).is_none() {
        table.alias(claimed, slot);
    }
}

/// Whether `slot` may be rewritten where it stands rather than left to what already names it.
///
/// Two conditions, and the batch has to satisfy both. Every element the slot is drawn through has
/// to be in this batch, or the ones that are not would silently take the colour of the ones that
/// are. And every one of them has to be moving it to the same colour, or the slot ends up holding
/// whichever of them was written last.
fn rewritable_in_place(
    covered: &FxHashMap<PaintSlot, Covering>,
    owners: &FxHashMap<PaintSlot, usize>,
    slot: PaintSlot,
) -> bool {
    covered.get(&slot).is_some_and(|covering| {
        covering.agreed && Some(covering.count) == owners.get(&slot).copied()
    })
}

/// Drops the key an element has stopped answering to, once no element still answers to it.
///
/// The count matters: siblings that cascaded to one result share the address, and forgetting it
/// while one of them still names it would send that one's next paragraph to a slot of its own,
/// which is a slot nothing rewrites when the colour next moves.
///
/// Returns the slot the forgotten key resolved to, if a key was forgotten.
fn retire(
    table: &mut TextPaintTable,
    slots: &TextSlots,
    previous: Option<TextSlot>,
    claimed: u64,
) -> Option<PaintSlot> {
    let previous = previous?;
    let address = address(&previous.key);
    if address == claimed || slots.held_by_anyone(address) {
        return None;
    }
    let unmapped = table.slot_of(address);
    table.forget(address);
    unmapped
}

/// Publishes how many records, slots in use and keys the brush tables hold.
pub(crate) fn publish(slots: &TextSlots, table: &TextPaintTable) {
    use zgui_profile::{Counter, counter};
    counter::set(Counter::TextSlotRecordsLive, slots.len() as u64);
    counter::set(Counter::TextBrushSlotsLive, table.in_use() as u64);
    counter::set(Counter::TextBrushKeysLive, table.keys() as u64);
}

/// The number the table files a cascade result under.
fn address(key: &TextPaintKey) -> u64 {
    key.addr() as u64
}

#[cfg(test)]
mod tests {
    use zgui_arena::{DomainId, Generation};
    use zgui_color::Color;
    use zgui_css::{PinnedGroup, StyleDraft};
    use zgui_dom::NodeKey;
    use zgui_scene::{PaintSlot, TextPaint, TextPaintTable};
    use zgui_style::{TextPaintUpdate, TextRun};
    use zgui_text_style::TextPaintKey;

    use super::{TextSlot, TextSlots, apply};

    /// A cascade result of its own, which is what one restyle of one element produces.
    fn cascade_result() -> TextPaintKey {
        PinnedGroup::inherited_text(&StyleDraft::initial().build())
    }

    /// The `n`th node of the first document.
    fn node(n: u32) -> NodeKey {
        NodeKey::new(n, Generation::FIRST, DomainId::FIRST)
    }

    /// A grey, so that two of them are told apart by one number.
    fn grey(level: f32) -> Color {
        Color::srgb(level, level, level, 1.0)
    }

    /// One element's update: its new cascade result, and the colour that result computed to.
    fn update(n: u32, color: Color) -> TextPaintUpdate {
        TextPaintUpdate {
            node: node(n),
            index: zgui_dom::NodeIndex::new(n),
            run: TextRun::Own,
            paint: zgui_text_style::TextPaint {
                key: cascade_result(),
                color,
            },
            restyled: false,
        }
    }

    /// Two elements drawn through one slot, which is what aliasing leaves behind.
    fn sharing_one_slot(color: Color) -> (TextSlots, TextPaintTable, PaintSlot) {
        let mut table = TextPaintTable::new();
        let shared = cascade_result();
        let slot = table.slot_for(super::address(&shared), || TextPaint::new(color));
        let mut slots = TextSlots::default();
        for n in [1, 2] {
            slots.insert(
                (node(n), TextRun::Own),
                TextSlot {
                    slot,
                    key: shared.clone(),
                },
            );
        }
        (slots, table, slot)
    }

    /// The colour an element is drawn in after a batch, read the way a shaped run reads it.
    fn drawn(slots: &TextSlots, table: &TextPaintTable, n: u32) -> TextPaint {
        let held = slots
            .get(&(node(n), TextRun::Own))
            .expect("the element was given a slot");
        *table.get(held.slot).expect("the slot exists")
    }

    /// The defect: two elements sharing a slot are re-coloured differently in one batch.
    ///
    /// Rewriting the slot where it stands writes it twice, and the second write is what both
    /// elements are then drawn in. Neither is told to re-shape, so nothing corrects it.
    #[test]
    fn two_elements_sharing_a_slot_that_diverge_are_not_rewritten_through_it() {
        let (mut slots, mut table, shared) = sharing_one_slot(grey(0.1));

        let split = apply(
            &mut slots,
            &mut table,
            &[update(1, grey(0.8)), update(2, grey(0.4))],
        )
        .split;

        assert_eq!(
            drawn(&slots, &table, 1),
            TextPaint::new(grey(0.8)),
            "the first element took the second's colour"
        );
        assert_eq!(
            drawn(&slots, &table, 2),
            TextPaint::new(grey(0.4)),
            "the second element took the first's colour"
        );
        assert_ne!(
            slots.get(&(node(1), TextRun::Own)).map(|held| held.slot),
            slots.get(&(node(2), TextRun::Own)).map(|held| held.slot),
            "two colours cannot be held by one slot"
        );
        let _ = shared;
        let mut named: Vec<u32> = split.iter().map(|(node, _)| node.index()).collect();
        named.sort_unstable();
        assert_eq!(
            named,
            vec![1, 2],
            "an element moved off the slot its glyphs name has to be shaped again"
        );
    }

    /// A cascade result that already has a slot keeps it, however the element arriving on it is
    /// drawn.
    ///
    /// The element here has been drawn through a slot of its own and has just cascaded onto a
    /// result that something else already claimed a slot against — which is what an element loses a
    /// colour declaration and falls back to an inherited one *is*. Both slots hold the same colour
    /// once this batch has run, so nothing on the screen moves; what must not move is the answer the
    /// table gives to the next run shaped under that result.
    #[test]
    fn a_cascade_result_that_already_has_a_slot_is_not_re_pointed_at_another() {
        let mut table = TextPaintTable::new();
        let inherited = cascade_result();
        let shared = table.slot_for(super::address(&inherited), || TextPaint::new(grey(0.1)));

        // The element's own slot, claimed against a result of its own and drawn in its own colour.
        let own_result = cascade_result();
        let own = table.slot_for(super::address(&own_result), || TextPaint::new(grey(0.5)));
        let mut slots = TextSlots::default();
        slots.insert(
            (node(1), TextRun::Own),
            TextSlot {
                slot: own,
                key: own_result,
            },
        );

        // It stops declaring a colour, so it cascades onto the inherited result and its colour
        // becomes that result's.
        let arriving = TextPaintUpdate {
            node: node(1),
            index: zgui_dom::NodeIndex::new(1),
            run: TextRun::Own,
            paint: zgui_text_style::TextPaint {
                key: inherited.clone(),
                color: grey(0.1),
            },
            restyled: true,
        };
        let split = apply(&mut slots, &mut table, &[arriving]).split;

        assert_eq!(
            slots.get(&(node(1), TextRun::Own)).map(|held| held.slot),
            Some(shared),
            "the element stayed on a slot its own result stopped answering to"
        );
        assert_eq!(
            split,
            vec![(node(1), TextRun::Own)],
            "the glyphs on the slot the element left have to be shaped again"
        );
        assert_eq!(
            table.slot_of(super::address(&inherited)),
            Some(shared),
            "the inherited result was re-pointed at the arriving element's slot, so everything \
             shaped under it afterwards is drawn in that element's colour"
        );
        assert_eq!(
            table.get(shared),
            Some(&TextPaint::new(grey(0.1))),
            "the result's own slot stopped holding the result's colour"
        );
    }

    /// Some of the elements sharing a slot leave it together, agreeing on where to.
    ///
    /// The slot is still drawn through by the elements that stayed, so it cannot be rewritten in
    /// place however many of the leavers agree — and the count that decides that has to be the
    /// count *before* the batch. Read as the batch moves records off the slot, it reaches the
    /// number of leavers on the last of them, and that one rewrites the slot under everyone else.
    #[test]
    fn leavers_that_agree_still_leave_a_slot_others_stay_on() {
        let mut table = TextPaintTable::new();
        let shared = cascade_result();
        let slot = table.slot_for(super::address(&shared), || TextPaint::new(grey(0.1)));
        let mut slots = TextSlots::default();
        for n in 1..=7 {
            slots.insert(
                (node(n), TextRun::Own),
                TextSlot {
                    slot,
                    key: shared.clone(),
                },
            );
        }

        let leaving = cascade_result();
        let updates: Vec<TextPaintUpdate> = (1..=4)
            .map(|n| TextPaintUpdate {
                node: node(n),
                index: zgui_dom::NodeIndex::new(n),
                run: TextRun::Own,
                paint: zgui_text_style::TextPaint {
                    key: leaving.clone(),
                    color: grey(0.9),
                },
                restyled: true,
            })
            .collect();
        let split = apply(&mut slots, &mut table, &updates).split;

        assert_eq!(
            table.get(slot),
            Some(&TextPaint::new(grey(0.1))),
            "the slot the three that stayed draw through was rewritten under them"
        );
        let mut named: Vec<u32> = split.iter().map(|(node, _)| node.index()).collect();
        named.sort_unstable();
        assert_eq!(
            named,
            vec![1, 2, 3, 4],
            "every leaver has to be shaped again"
        );
        for n in 1..=4 {
            assert_eq!(drawn(&slots, &table, n), TextPaint::new(grey(0.9)));
        }
        for n in 5..=7 {
            assert_eq!(drawn(&slots, &table, n), TextPaint::new(grey(0.1)));
        }
    }

    /// One element's update: an arriving cascade result and the colour it computed to.
    fn arriving(n: u32, key: &TextPaintKey, color: Color) -> TextPaintUpdate {
        TextPaintUpdate {
            node: node(n),
            index: zgui_dom::NodeIndex::new(n),
            run: TextRun::Own,
            paint: zgui_text_style::TextPaint {
                key: key.clone(),
                color,
            },
            restyled: true,
        }
    }

    /// An element that cascades onto a result another element already claimed a slot against.
    ///
    /// The result owns that slot: every paragraph flattened under it from now on is drawn through
    /// it. So the element is moved onto it and its standing glyphs — which name the slot it is
    /// leaving — are reported as shaping that has to be done again.
    #[test]
    fn an_element_that_cascades_onto_a_claimed_result_moves_onto_its_slot() {
        let mut table = TextPaintTable::new();
        let mut slots = TextSlots::default();
        let shared = cascade_result();
        let own = cascade_result();
        apply(
            &mut slots,
            &mut table,
            &[
                arriving(1, &shared, grey(0.1)),
                arriving(2, &own, grey(0.1)),
            ],
        );
        let claimed = table
            .slot_of(super::address(&shared))
            .expect("the first element claimed a slot");

        let split = apply(&mut slots, &mut table, &[arriving(2, &shared, grey(0.1))]).split;

        assert_eq!(
            slots.get(&(node(2), TextRun::Own)).map(|held| held.slot),
            Some(claimed),
            "the element is drawn through a slot its own cascade result does not answer to"
        );
        assert_eq!(
            split,
            vec![(node(2), TextRun::Own)],
            "glyphs that name the slot the element left have to be shaped again"
        );
    }

    /// A slot two cascade results answer to is left to whichever element stays on it.
    ///
    /// The second element here arrived on the first's result and joined its slot. Re-colouring the
    /// first alone therefore cannot be a write through that slot, however few records name it: the
    /// result the second still cascades to answers to it, and every string flattened under that
    /// result afterwards reads whatever the write left behind.
    #[test]
    fn a_result_a_second_element_still_holds_keeps_the_colour_its_slot_holds() {
        let mut table = TextPaintTable::new();
        let mut slots = TextSlots::default();
        let shared = cascade_result();
        let own = cascade_result();
        apply(
            &mut slots,
            &mut table,
            &[
                arriving(1, &shared, grey(0.1)),
                arriving(2, &own, grey(0.1)),
            ],
        );
        apply(&mut slots, &mut table, &[arriving(2, &shared, grey(0.1))]);

        let moved = cascade_result();
        apply(&mut slots, &mut table, &[arriving(1, &moved, grey(0.8))]);

        let answers = table
            .slot_of(super::address(&shared))
            .expect("the result still answers to a slot");
        assert_eq!(
            table.get(answers),
            Some(&TextPaint::new(grey(0.1))),
            "a run flattened under the shared result would be shaped into the other colour"
        );
        assert_eq!(drawn(&slots, &table, 1), TextPaint::new(grey(0.8)));
        assert_eq!(drawn(&slots, &table, 2), TextPaint::new(grey(0.1)));
    }

    /// The path the divergence guard must not cost anything: a theme flip, where every element
    /// drawn through a slot moves to the same new colour.
    #[test]
    fn two_elements_sharing_a_slot_that_agree_are_rewritten_through_it() {
        let (mut slots, mut table, shared) = sharing_one_slot(grey(0.1));

        let split = apply(
            &mut slots,
            &mut table,
            &[update(1, grey(0.9)), update(2, grey(0.9))],
        )
        .split;

        assert!(
            split.is_empty(),
            "a colour that can be written through the slot costs no shaping"
        );
        for n in [1, 2] {
            assert_eq!(
                slots.get(&(node(n), TextRun::Own)).map(|held| held.slot),
                Some(shared)
            );
            assert_eq!(drawn(&slots, &table, n), TextPaint::new(grey(0.9)));
        }
    }

    /// Siblings that cascaded to one result share its slot. When most of them leave the document,
    /// the one that stays still answers to that result, so the sweep keeps the key and the slot,
    /// and a later claim takes a slot of its own.
    #[test]
    fn a_sweep_keeps_the_slot_of_a_result_a_survivor_still_holds() {
        let mut table = TextPaintTable::new();
        let mut slots = TextSlots::default();
        let shared = cascade_result();
        let siblings = super::SWEEP_AFTER_DROPPED as u32 + 8;
        let arrivals: Vec<_> = (1..=siblings)
            .map(|n| arriving(n, &shared, grey(0.1)))
            .collect();
        apply(&mut slots, &mut table, &arrivals);
        let kept = slots
            .get(&(node(1), TextRun::Own))
            .expect("the survivor was given a slot")
            .slot;

        let released = slots.sweep(&mut table, u64::from(siblings), |key| key == node(1));
        assert!(
            released.is_empty(),
            "the survivor is still drawn through {kept:?}"
        );
        assert_eq!(slots.len(), 1);
        assert_eq!(table.slot_of(super::address(&shared)), Some(kept));

        let other = cascade_result();
        apply(&mut slots, &mut table, &[arriving(500, &other, grey(0.8))]);
        assert_ne!(drawn(&slots, &table, 500), drawn(&slots, &table, 1));
        assert_eq!(
            slots.get(&(node(1), TextRun::Own)).map(|held| held.slot),
            Some(kept)
        );
        assert_eq!(drawn(&slots, &table, 1), TextPaint::new(grey(0.1)));
    }
}
