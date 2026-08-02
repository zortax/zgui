//! One layer on the stack, and what it is still answering for.

use zgui::prelude::*;

use crate::dismiss::stack::LayerId;

/// One registered layer: what it is called, where it sits, and what it still answers.
pub(crate) struct Entry {
    /// What it is called.
    id: LayerId,
    /// Which band it is on, and when it went up — which is the order layers answer in.
    rank: (OverlayLayer, u64),
    /// The layer's own element, which is what says whether it is still on the screen.
    surface: NodeRef,
    /// Whether that element has ever been bound.
    ///
    /// The difference between a layer that has not been built yet and one that has been taken
    /// away: an unbound handle reads the same either way, and only one of the two should stop
    /// answering.
    mounted: bool,
    /// Whether the surface has been asked to close and is playing its exit.
    leaving: bool,
}

impl Entry {
    /// A layer on `band`, `ordinal`-th to go up, over the element `surface` names.
    pub(crate) fn new(id: LayerId, band: OverlayLayer, ordinal: u64, surface: NodeRef) -> Self {
        Self {
            id,
            rank: (band, ordinal),
            surface,
            mounted: false,
            leaving: false,
        }
    }

    /// What it is called.
    pub(crate) fn id(&self) -> LayerId {
        self.id
    }

    /// Where it sits: the band first, and the order it opened in second.
    pub(crate) fn rank(&self) -> (OverlayLayer, u64) {
        self.rank
    }

    /// Whether it has been asked to close and is playing its exit.
    pub(crate) fn leaving(&self) -> bool {
        self.leaving
    }

    /// Records whether it has been asked to close.
    pub(crate) fn set_leaving(&mut self, leaving: bool) {
        self.leaving = leaving;
    }

    /// Whether the layer is still on the screen, recording that it has been seen there.
    ///
    /// A layer that was mounted and is gone answers nothing ever again. One that has never been
    /// mounted still answers: it was registered moments ago by a component whose element the view
    /// has not bound yet, and refusing it would drop the first press or key after every open.
    pub(crate) fn live(&mut self) -> bool {
        if self.surface.get_untracked().is_some() {
            self.mounted = true;
            return true;
        }
        !self.mounted
    }

    /// How this reads in a trace line: its name, its band, and what it is doing.
    pub(crate) fn describe(&mut self) -> String {
        let state = match (self.live(), self.leaving) {
            (false, _) => ":gone",
            (true, true) => ":leaving",
            (true, false) => "",
        };
        format!("{}:{}{state}", self.id.get(), self.rank.0.name())
    }
}
