//! From the face a shaped run names to the handle the rest of the pipeline uses.

use skrifa::attribute::Style;
use skrifa::{FontRef, MetadataProvider};
use zgui_interned::Ident;
use zgui_text::{FaceId, FaceRecord};
use zgui_text_style::{DEFAULT_OBLIQUE_DEGREES, FontSlant};

use crate::system::FontSystem;

/// This framework's spelling of a face's own declared slant.
fn slant_of(style: Style) -> FontSlant {
    match style {
        Style::Normal => FontSlant::Upright,
        Style::Italic => FontSlant::Italic,
        Style::Oblique(degrees) => FontSlant::Oblique(degrees.unwrap_or(DEFAULT_OBLIQUE_DEGREES)),
    }
}

impl FontSystem {
    /// The handle for the face a shaped run was drawn with.
    ///
    /// A shaped run names its face by the file it lives in and its index within that file, because
    /// that is what the shaper resolved — including for a character that fell through to a
    /// fallback family the run never asked for. Painting has to get from that back to a handle it
    /// can key rasterised glyphs by, and this is the whole of that step.
    ///
    /// A face already known here keeps the handle it already has, so a glyph rasterised for a run
    /// and the same glyph rasterised for a resolved query share one cache entry. A face reached
    /// only through fallback has never been registered under a name, so the record issued for it
    /// carries no family and its own axis values, read from the file.
    pub fn face_for(&self, font: &parley::FontData) -> FaceId {
        let (blob, index) = (font.data.clone(), font.index);
        self.locked(|shared| {
            let colors = shared.color_support((blob.id(), index), blob.data());
            let face = FontRef::from_index(blob.data(), index).ok();
            let attributes = face.as_ref().map(MetadataProvider::attributes);
            let weight = attributes.as_ref().map_or(400.0, |it| it.weight.value());
            let slant = attributes
                .as_ref()
                .map_or(FontSlant::Upright, |it| slant_of(it.style));
            let width = attributes.as_ref().map_or(1.0, |it| it.stretch.ratio());
            let variable = face.as_ref().is_some_and(|face| !face.axes().is_empty());
            shared
                .faces
                .intern(Ident::new(""), None, blob, index, |id| FaceRecord {
                    id,
                    family: Ident::new(""),
                    weight,
                    slant,
                    width,
                    is_variable: variable,
                    has_color: colors.any(),
                })
        })
    }
}
