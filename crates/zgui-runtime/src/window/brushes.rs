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

/// Which slot one element's text is drawn through.
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
pub(crate) fn apply(
    slots: &mut FxHashMap<SlotKey, TextSlot>,
    table: &mut TextPaintTable,
    updates: &[TextPaintUpdate],
) -> Vec<SlotKey> {
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
    let mut owners: FxHashMap<PaintSlot, usize> = FxHashMap::default();
    let mut holders: FxHashMap<u64, usize> = FxHashMap::default();
    for held in slots.values() {
        *owners.entry(held.slot).or_default() += 1;
        *holders.entry(address(&held.key)).or_default() += 1;
    }

    let mut split = Vec::new();
    for (update, moved) in updates.iter().zip(&recoloured) {
        let paint = TextPaint::new(update.paint.color);
        let key = update.paint.key.clone();
        let claimed = address(&key);
        let held = slots.get(&(update.node, update.run));
        let slot = match held {
            Some(held) if !*moved => {
                // Nothing is drawn differently, so nothing is rewritten and nothing is re-shaped.
                let slot = held.slot;
                establish(table, claimed, slot);
                slot
            }
            Some(held) if rewritable_in_place(&covered, &owners, held.slot) => {
                let slot = held.slot;
                table.set(slot, paint);
                // The new cascade result is pointed at the slot everything already shaped still
                // names, so that the next paragraph flattened under the new colour does not claim
                // a second one and draw the two halves of one element in two colours. Only where
                // the result has no slot of its own: see [`establish`] for what taking one costs.
                establish(table, claimed, slot);
                slot
            }
            other => {
                // An element leaving a slot that other elements still use cannot be re-coloured by
                // writing through it, and what its glyphs name was baked in when they were shaped.
                // The shaping is what has to go.
                if other.is_some() {
                    split.push((update.node, update.run));
                }
                let slot = table.slot_for(claimed, || paint);
                table.set(slot, paint);
                slot
            }
        };
        *holders.entry(claimed).or_default() += 1;
        let previous = slots.insert((update.node, update.run), TextSlot { slot, key });
        retire(table, &mut holders, previous, claimed);
    }
    split
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
fn retire(
    table: &mut TextPaintTable,
    holders: &mut FxHashMap<u64, usize>,
    previous: Option<TextSlot>,
    claimed: u64,
) {
    let Some(previous) = previous else {
        return;
    };
    let address = address(&previous.key);
    if address == claimed {
        return;
    }
    let Some(count) = holders.get_mut(&address) else {
        return;
    };
    *count -= 1;
    if *count == 0 {
        holders.remove(&address);
        table.forget(address);
    }
}

/// The number the table files a cascade result under.
fn address(key: &TextPaintKey) -> u64 {
    key.addr() as u64
}

#[cfg(test)]
mod tests {
    use rustc_hash::FxHashMap;
    use zgui_arena::{DomainId, Generation};
    use zgui_color::Color;
    use zgui_css::{PinnedGroup, StyleDraft};
    use zgui_dom::NodeKey;
    use zgui_scene::{PaintSlot, TextPaint, TextPaintTable};
    use zgui_style::{TextPaintUpdate, TextRun};
    use zgui_text_style::TextPaintKey;

    use super::{SlotKey, TextSlot, apply};

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
        }
    }

    /// Two elements drawn through one slot, which is what aliasing leaves behind.
    fn sharing_one_slot(
        color: Color,
    ) -> (FxHashMap<SlotKey, TextSlot>, TextPaintTable, PaintSlot) {
        let mut table = TextPaintTable::new();
        let shared = cascade_result();
        let slot = table.slot_for(super::address(&shared), || TextPaint::new(color));
        let mut slots: FxHashMap<SlotKey, TextSlot> = FxHashMap::default();
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
    fn drawn(
        slots: &FxHashMap<SlotKey, TextSlot>,
        table: &TextPaintTable,
        n: u32,
    ) -> TextPaint {
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
        );

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
            slots[&(node(1), TextRun::Own)].slot,
            slots[&(node(2), TextRun::Own)].slot,
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
        let mut slots: FxHashMap<SlotKey, TextSlot> = FxHashMap::default();
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
        };
        apply(&mut slots, &mut table, &[arriving]);

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

    /// The path the divergence guard must not cost anything: a theme flip, where every element
    /// drawn through a slot moves to the same new colour.
    #[test]
    fn two_elements_sharing_a_slot_that_agree_are_rewritten_through_it() {
        let (mut slots, mut table, shared) = sharing_one_slot(grey(0.1));

        let split = apply(
            &mut slots,
            &mut table,
            &[update(1, grey(0.9)), update(2, grey(0.9))],
        );

        assert!(
            split.is_empty(),
            "a colour that can be written through the slot costs no shaping"
        );
        for n in [1, 2] {
            assert_eq!(slots[&(node(n), TextRun::Own)].slot, shared);
            assert_eq!(drawn(&slots, &table, n), TextPaint::new(grey(0.9)));
        }
    }
}
