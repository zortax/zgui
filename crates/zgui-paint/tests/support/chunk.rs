//! One chunk of emitted items, recorded into a compositor of its own and drawn.

use iceblink::ChunkKey;
use zgui_paint::Tables;
use zgui_paint::emit::Recorder;
use zgui_space::{ClipId, SpatialId};
use zgui_testkit_compositor::{Drawing, Session};

/// The key the chunk is recorded under.
const KEY: ChunkKey = ChunkKey::new(1, 0);

/// A compositor drawing one chunk.
pub(crate) struct Lone {
    session: Session,
}

impl Lone {
    /// Records what `emit` records as the only chunk of a 256 by 256 output, with `space` and
    /// `clip` as its defaults, and commits it.
    pub(crate) fn record(
        scene: &mut Tables,
        defaults: (SpatialId, ClipId),
        emit: impl FnOnce(&mut Recorder<'_>),
    ) -> Self {
        let mut session = Session::new([256, 256]).unwrap_or_else(|why| panic!("{why}"));
        let mut transaction = session.transaction();
        zgui_paint::record_chunk(&mut transaction, KEY, scene, defaults, emit);
        session.show(&mut transaction, &[KEY]);
        session.commit(transaction);
        Self { session }
    }

    /// Moves the chunk by `offset` from where it was recorded.
    pub(crate) fn offset(&mut self, offset: [f32; 2]) {
        let mut transaction = self.session.transaction();
        transaction.set_offset(KEY, offset);
        self.session.commit(transaction);
    }

    /// Draws the whole output and returns what it draws.
    pub(crate) fn redraw(&mut self) -> Drawing {
        self.session.redraw()
    }
}
