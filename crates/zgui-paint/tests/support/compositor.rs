//! A harness's frames recorded into a compositor from the test factory.

use iceblink::{Built, CommitCounters};
use zgui_paint::{CompositorSync, SyncReport};
use zgui_testkit_compositor::TestOutput;

use super::Harness;

/// One output that a harness's painter records its frames into.
pub(crate) struct Recorded {
    /// The compositor, its renderer and the output.
    pub(crate) output: TestOutput,
}

/// What one recorded frame wrote and drew.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Written {
    /// What the sync wrote into the transaction.
    pub(crate) report: SyncReport,
    /// What the compositor applied at commit.
    pub(crate) commit: CommitCounters,
    /// Whether the compositor built a frame.
    pub(crate) built: Built,
}

impl Recorded {
    /// Returns an output of `size` device pixels, or `None` with the reason printed when the
    /// factory cannot provide one.
    pub(crate) fn new(size: [u32; 2]) -> Option<Self> {
        match TestOutput::new(size) {
            Ok(output) => Some(Self { output }),
            Err(why) => {
                eprintln!("skipped: {why}");
                None
            }
        }
    }

    /// Retires the fragments the store dropped, paints what the harness's damage reaches, commits
    /// the transaction and draws the frame.
    ///
    /// The damage is cleared afterwards, as the end of a frame clears it.
    pub(crate) fn frame(&mut self, harness: &mut Harness) -> Written {
        if harness.painter.compositor().is_none() {
            harness
                .painter
                .attach_compositor(CompositorSync::new(self.output.output()));
        }
        let transaction = self.output.transaction();
        harness
            .painter
            .compositor()
            .expect("attached above")
            .open(transaction);
        let retired = harness.store.drain_retired_paint();
        harness.painter.retire(&retired, &mut harness.scene);
        harness.paint();
        let sync = harness.painter.compositor().expect("attached above");
        let transaction = sync
            .close(&harness.store, &mut harness.scene)
            .expect("opened above");
        let report = sync.report();
        let commit = self.output.commit(transaction).counters;
        let built = self.output.draw();
        harness.clear_damage();
        Written {
            report,
            commit,
            built,
        }
    }

    /// The same, with the whole surface damaged.
    pub(crate) fn frame_everything(&mut self, harness: &mut Harness) -> Written {
        harness.damage = zgui_bits::DamageSet::full();
        self.frame(harness)
    }
}
