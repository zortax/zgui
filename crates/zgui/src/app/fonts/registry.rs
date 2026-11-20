//! The collection as the compositor's font registry, opened when first asked.

use std::sync::{Arc, OnceLock};

use compositor_api::FontRegistry;
use compositor_types::{FontData, FontId};
use zgui_text_parley::CompositorFonts;

use crate::app::fonts::pending::Pending;

/// A font registry over a collection that may still be enumerating the machine's faces.
///
/// The first question waits for the collection.
#[derive(Debug)]
pub(crate) struct Registry {
    system: Arc<Pending>,
    fonts: OnceLock<CompositorFonts>,
}

impl Registry {
    /// Returns the registry of `system`'s faces.
    pub(crate) fn new(system: Arc<Pending>) -> Self {
        Self {
            system,
            fonts: OnceLock::new(),
        }
    }

    fn fonts(&self) -> &CompositorFonts {
        self.fonts
            .get_or_init(|| CompositorFonts::new(Arc::clone(self.system.get())))
    }
}

impl FontRegistry for Registry {
    fn register(&self, data: FontData) -> FontId {
        self.fonts().register(data)
    }

    fn get(&self, id: FontId) -> Option<FontData> {
        self.fonts().get(id)
    }

    fn generation(&self) -> u64 {
        self.fonts().generation()
    }
}
