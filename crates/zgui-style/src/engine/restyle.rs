//! Styling a document once, and turning what that did into what the rest of the frame owes.
//!
//! One entry point covers both halves, because they cannot be separated. The damage has to be read
//! out of the elements the traversal touched while it still describes them; the root-metrics
//! fixpoint may run the traversal a second time; and the obligations the traversal consumed are
//! retired here by an explicit walk, because the engine owns the traversal and no other phase's
//! walk drains them as a side effect of doing its own work.

use style::invalidation::element::restyle_hints::RestyleHint;
use style::traversal_flags::TraversalFlags;
use zgui_bits::Dirty;
use zgui_dom::dirty::{propagate, walk};
use zgui_dom::{Document, NodeIndex, NodeKey};
use zgui_profile::{Counter, counter};
use zgui_text_style::TextPaint;
use zgui_text_style::lower::paint::paint;

use crate::damage::{DamageSink, translate};
use crate::driver::traversal::Restyled;
use crate::driver::{self, Restyle};
use crate::engine::StyleEngine;
use crate::engine::stylist;
use crate::engine::thread_pool::{self, StylePool};

/// Which of the runs an element is the source of an update is about.
///
/// An element is the source of more than one: its own content, and the content its `::before` and
/// `::after` generate. Each is cascaded separately and each therefore holds its *own* colour — a
/// placeholder is `muted-foreground` while the field around it is `foreground` — so each claims a
/// brush slot of its own. Naming which one an update is about is what keeps them apart: without it
/// an element has one remembered slot for several, only one of them is ever rewritten, and the
/// others keep the colour they were shaped in for as long as the shaping survives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TextRun {
    /// The element's own content.
    Own,
    /// What its `::before` generates.
    Before,
    /// What its `::after` generates.
    After,
}

/// One run whose text colour changed, for whoever owns the table the colour is looked up in.
///
/// A shaped paragraph stores an *index* into a paint table rather than a colour, precisely so that
/// changing a theme rewrites a handful of table entries instead of re-shaping every string in the
/// application. Producing the list of changed entries is this crate's job; the table itself belongs
/// to the scene, which this crate does not depend on, so the list crosses as data.
#[derive(Clone, Debug)]
pub struct TextPaintUpdate {
    /// The element the run belongs to.
    pub node: NodeKey,
    /// Its slot, for a consumer that is walking the document rather than a table.
    pub index: NodeIndex,
    /// Which of that element's runs it is.
    pub run: TextRun,
    /// What the text is now drawn in.
    pub paint: TextPaint,
}

impl StyleEngine {
    /// Styles `document`, translates what that damaged, and retires the obligations it serviced.
    ///
    /// Returns what the pass did: how many elements it styled, how many of those it matched
    /// against the rule set rather than merely cascading again, and how many were given an
    /// obligation as a result.
    pub fn restyle(&mut self, document: &mut Document, pool: Option<&StylePool>) -> Restyle {
        self.text_paint_updates.clear();
        if !self.needs_restyle(document) {
            return Restyle::default();
        }

        let snapshots = crate::driver::snapshots::RestyleSnapshots::take(document);
        let mut report = Restyle {
            snapshots: snapshots.len(),
            ..Restyle::default()
        };
        let pool = pool.map(StylePool::pool);
        let mut sink = DamageSink::new();

        thread_pool::as_layout_thread(|| {
            // The animation-only traversal, first and separately, because the ordinary one will
            // not do its work: an element whose animation asked for a cascade carries a hint that
            // only this traversal processes, and the ordinary traversal asserts when it finds one
            // still outstanding. It runs before the ordinary passes, so what it computes is the
            // input the rest of the frame — including a descendant that inherits from it — is
            // styled and laid out against.
            if std::mem::take(&mut self.animation_restyle_owed) {
                let (records, workers, traversed, time) = driver::run_pass(
                    &mut self.stylist,
                    &self.lock,
                    driver::Pass {
                        document,
                        snapshots: snapshots.map(),
                        pool,
                        animations: self.animations.shared(),
                        now: self.animations.now(),
                    },
                    TraversalFlags::AnimationOnly,
                );
                report.animation_pass = traversed;
                report.traversed |= traversed;
                report.workers = report.workers.max(workers);
                report.engine_time += time;

                self.translate_records(document, &records, &mut sink);
                clear_restyle_state(document, &records);
                absorb(&mut report, records);
            }

            // At most two passes: the second exists only for the root-metrics fixpoint, and a
            // third could not converge on anything the second did not.
            for pass in 0..2u8 {
                let (records, workers, traversed, time) = driver::run_pass(
                    &mut self.stylist,
                    &self.lock,
                    driver::Pass {
                        document,
                        snapshots: snapshots.map(),
                        pool,
                        animations: self.animations.shared(),
                        now: self.animations.now(),
                    },
                    TraversalFlags::empty(),
                );
                report.passes += 1;
                report.traversed |= traversed;
                report.workers = report.workers.max(workers);
                report.engine_time += time;

                self.translate_records(document, &records, &mut sink);
                clear_restyle_state(document, &records);
                absorb(&mut report, records);

                if !stylist::push_root_metrics(&self.stylist, document, &mut self.root_metrics) {
                    break;
                }
                if pass == 1 {
                    tracing::warn!(
                        "the root font metrics did not settle in two passes; the frame is styled \
                         against the second pass's values"
                    );
                    break;
                }
                mark_root_for_recascade(document);
            }
        });

        report.damaged = sink.len();
        sink.apply(document.store_mut());
        snapshots.finish(document.store());
        self.stylist.rule_tree().maybe_gc();

        // The one phase at which the dependency index describes the sheets that are installed: the
        // rule set has been flushed by every pass above, and nothing else will flush it this frame.
        if self.deps.is_disabled() {
            self.deps.rebuild(&self.stylist);
        }

        // The keys of elements the document no longer holds, which nothing else would ever drop.
        self.texts.retire(document.store());

        // Retire what the traversal consumed. Every other phase's walk retires its own bits as a
        // side effect of doing its work; this pair's does not exist, because the engine owns the
        // traversal.
        let from = document.document_index();
        walk::walk(
            document.store_mut(),
            from,
            Dirty::RESTYLE | Dirty::RECASCADE,
            &mut |_store, _node| {},
        );

        // The two counters mean two different amounts of work: matching an element against the
        // rule set, and re-running its cascade with the matches it already had.
        counter::add(Counter::ElementsRestyled, report.matched as u64);
        counter::add(
            Counter::ElementsRecascaded,
            (report.styled - report.matched) as u64,
        );
        report
    }

    /// Turns one pass's records into obligations, and collects the text colours that moved.
    fn translate_records(
        &mut self,
        document: &mut Document,
        records: &[Restyled],
        sink: &mut DamageSink,
    ) {
        for record in records {
            let Some(style) = document.node(record.index).primary_style() else {
                continue;
            };
            // Read before the translation replaces it: the comparison the translation makes is the
            // same one this needs, and doing it twice would be two answers to one question.
            let held = document.store().columns().paint_key.get(record.node);
            let previous_text = held.map(|key| key.inherited_text);
            let previous_pseudos = held.map(|key| [key.pseudo_before, key.pseudo_after]);

            // The same number the key carries, by the same route: an identity computed two ways is
            // two identities, and every restyled element would report a colour it did not change.
            let now = crate::damage::paint_key::inherited_text(&style);

            // Read before the translation as well, and through the same accessors box building
            // uses, so that a pseudo-element which generates nothing is absent here exactly when it
            // has no box.
            let generated = [
                document.node(record.index).before_style(),
                document.node(record.index).after_style(),
            ];

            translate(document.store_mut(), &mut self.texts, record, &style, sink);

            if previous_text != Some(now) {
                self.text_paint_updates.push(TextPaintUpdate {
                    node: record.node,
                    index: record.index,
                    run: TextRun::Own,
                    paint: paint(&style),
                });
            }

            // Generated content cascades on its own and holds its own colour, and the identity of
            // that cascade is the only thing recorded about it. So a pseudo-element whose result
            // moved at all is reported, which over-reports a colour that did not change and never
            // misses one that did — the same bargain every other field of the key strikes.
            for (slot, run) in [TextRun::Before, TextRun::After].into_iter().enumerate() {
                let Some(pseudo) = generated[slot].as_ref() else {
                    continue;
                };
                let moved = previous_pseudos.is_none_or(|held| {
                    held[slot] != crate::damage::paint_key::pseudo_identity(Some(pseudo))
                });
                if moved {
                    self.text_paint_updates.push(TextPaintUpdate {
                        node: record.node,
                        index: record.index,
                        run,
                        paint: paint(pseudo),
                    });
                }
            }
        }
    }
}

/// Clears the per-element restyle bookkeeping one pass left behind.
///
/// The engine leaves its flags and its damage set so that an embedder can turn them into its own
/// invalidation, which is exactly what has just happened; a pass that did not clear them would
/// report the previous pass's answer for ever.
fn clear_restyle_state(document: &Document, records: &[Restyled]) {
    for record in records {
        if let Some(mut data) = document.node(record.index).mutate_style_data() {
            data.clear_restyle_flags_and_damage();
        }
    }
}

/// Records that the whole document has to cascade again, for the root-metrics fixpoint.
///
/// Both halves are needed and neither is enough. The hint is what the engine reads to decide how
/// much of each element to redo; the mark is what makes the traversal descend to it at all.
fn mark_root_for_recascade(document: &mut Document) {
    let Some(root) = document.root_index() else {
        return;
    };
    if let Some(mut data) = document.node(root).mutate_style_data() {
        data.hint.insert(RestyleHint::recascade_subtree());
    }
    propagate::mark(document.store_mut(), root, Dirty::RECASCADE);
}

/// Folds one pass's records into the report.
fn absorb(report: &mut Restyle, records: Vec<Restyled>) {
    for record in &records {
        report.styled += 1;
        if !record.initial {
            report.restyled += 1;
        }
        if record.matched {
            report.matched += 1;
        }
    }
    report.records.extend(records);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use zgui_dom::{Document, EverythingMatters, NodeKind};
    use zgui_geom::CssPx;
    use zgui_interned::{ClassName, ElementName};
    use zgui_text::FixedMetrics;

    use crate::device::Viewport;
    use crate::engine::StyleEngine;
    use crate::sheets::SheetSource;
    use crate::sheets::origin::SheetOrigin;

    /// A document with one root element and an engine over it.
    fn engine() -> (Document, StyleEngine) {
        let mut document = Document::new();
        document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let engine = StyleEngine::new(
            &document,
            Arc::new(FixedMetrics::new()),
            Viewport::new(CssPx(800.0), CssPx(600.0)),
        );
        (document, engine)
    }

    #[test]
    fn the_text_key_store_does_not_grow_with_every_element_the_document_has_ever_held() {
        let (mut document, mut engine) = engine();
        engine.add_sheet(
            &document,
            SheetOrigin::Author,
            SheetSource::Text("box { font-size: 10px } .big { font-size: 40px }"),
        );
        let root = document.root_index().expect("the root element");

        // Well past the floor, so the sweep is reachable at all. Each round styles a row, moves a
        // property the shaper reads so that the row earns a key, and then removes it — which is the
        // shape a virtualised list has, and the shape that grows a store that never forgets.
        const ROUNDS: usize = 600;
        let mut ever_held = 0;
        for _ in 0..ROUNDS {
            let row = document
                .edit(&EverythingMatters, |batch| {
                    let row = batch.create_element(ElementName::new("box"));
                    batch.insert_before(root, row, None);
                    row
                })
                .expect("an unpoisoned document");
            engine.restyle(&mut document, None);

            document
                .edit(&EverythingMatters, |batch| {
                    batch.set_classes(row, &[ClassName::new("big")]);
                })
                .expect("an unpoisoned document");
            engine.restyle(&mut document, None);
            ever_held = ever_held.max(engine.texts.len());

            document
                .edit(&EverythingMatters, |batch| batch.remove(row))
                .expect("an unpoisoned document");
            zgui_dom::arena::end_frame(&mut document);
        }
        engine.restyle(&mut document, None);

        // The control first, because the bound below is worthless without it: every round really
        // did put a key in the store, so a store that swept nothing would end holding all of them.
        assert!(
            ever_held > 1,
            "no round recorded a text key, so the bound below measures nothing"
        );

        let held = engine.texts.len();
        assert!(
            held < ROUNDS / 4,
            "{held} keys for a document of three nodes: the sweep is not keeping up with the churn"
        );
    }
}
