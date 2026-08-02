//! The faces an application draws with, and the three seams they reach the frame through.

mod pending;

use std::sync::Arc;

use zgui_text::{FontData, FontError, FontSource};
use zgui_text_parley::{FontSystem, FontSystemOptions, Rasteriser, Shaper};

use crate::app::fonts::pending::Pending;

pub use crate::app::fonts::pending::system_collections_built;

/// The faces one application draws with.
///
/// One collection serves every window and all three of the things a frame asks about text —
/// what a face measures, how a paragraph is shaped, and what a glyph looks like as pixels — so
/// that a face registered once is visible to all of them and a metric answered once is not
/// answered again.
///
/// ```
/// use zgui::app::Fonts;
///
/// // The faces installed on this machine, plus whatever the application registers itself.
/// let fonts = Fonts::system();
/// assert!(fonts.register(std::sync::Arc::new(*b"not a font"), None).is_err());
/// ```
///
/// [`FontError`]: zgui_text::FontError
#[derive(Clone, Debug)]
pub struct Fonts {
    /// The collection, shared with everything built from it, once it exists.
    system: Arc<Pending>,
}

impl Default for Fonts {
    fn default() -> Self {
        Self::system()
    }
}

impl Fonts {
    /// The faces installed on this machine.
    ///
    /// Enumerating them is started here and finished the first time anything asks a question about
    /// a face, so an application that names these faces before it has a window pays for them
    /// alongside opening its graphics device rather than in front of it.
    pub fn system() -> Self {
        Self {
            system: Arc::new(Pending::system()),
        }
    }

    /// Only the faces the application registers itself.
    ///
    /// What is installed differs between machines, so an application that ships its own faces and
    /// enumerates nothing looks the same everywhere. An application that registers no face at all
    /// this way draws no text at all, which is the honest outcome of asking for exactly that.
    pub fn shipped_only() -> Self {
        Self {
            system: Arc::new(Pending::ready(FontSystem::new(
                FontSystemOptions::registered_only(),
            ))),
        }
    }

    /// Adds every face in one font file, optionally under a family name of the application's own.
    ///
    /// The name is what `font-family` in a style sheet has to say to reach these faces. With no
    /// name they are reachable under whatever family the file itself declares.
    ///
    /// # Errors
    ///
    /// Returns [`FontError`] when the bytes are not a font file this engine can read.
    pub fn register(&self, data: FontData, family: Option<&str>) -> Result<(), FontError> {
        self.system
            .get()
            .register(data, family.map(zgui_view::Ident::new))
            .map(|_| ())
    }

    /// What answers the cascade's font-metric questions.
    pub fn metrics(&self) -> Arc<dyn zgui_text::FontMetricsSource> {
        Arc::clone(self.system.get()) as Arc<dyn zgui_text::FontMetricsSource>
    }

    /// A shaper over these faces.
    pub fn shaper(&self) -> Shaper {
        Shaper::new(Arc::clone(self.system.get()))
    }

    /// What turns these faces' glyphs into pixels.
    pub fn raster(&self) -> Arc<dyn zgui_text::GlyphRaster> {
        Arc::new(Rasteriser::new(Arc::clone(self.system.get()))) as Arc<dyn zgui_text::GlyphRaster>
    }
}
