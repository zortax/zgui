//! Shared fixtures: the shipped faces, and a paragraph built over them.

// The fixtures are compiled into six test targets and each uses a different part of them — the
// raster target never builds a bidirectional paragraph and the metrics target never shapes one. A
// helper unused by one of them is not dead.
#![allow(dead_code, unreachable_pub)]

use std::sync::Arc;

use zgui_geom::CssPx;
use zgui_scene::PaintSlot;
use zgui_text::{FontSource, ParagraphContent, StyledRun, TextMap};
use zgui_text_parley::{Controls, FontSystem, FontSystemOptions, Shaper};
use zgui_text_style::{
    Direction, FamilyName, FontFamilyList, GenericFamily, ParagraphStyle, TextAlign, TextStyle,
};

/// The Latin face shipped with these tests.
pub const LATIN: &str = "Noto Sans";

/// The Arabic face shipped with these tests.
pub const ARABIC: &str = "Noto Sans Arabic";

/// The colour face shipped with these tests, which carries layered colour outlines.
pub const COLOR: &str = "Noto Znamenny Musical Notation";

/// Reads one of the shipped faces.
pub fn face(file: &str) -> Arc<dyn AsRef<[u8]> + Send + Sync> {
    let path = format!("{}/tests/fonts/{file}", env!("CARGO_MANIFEST_DIR"));
    Arc::new(std::fs::read(&path).unwrap_or_else(|error| panic!("reading {path}: {error}")))
}

/// A font system holding the Latin and Arabic faces and nothing the machine happens to have.
pub fn fonts() -> Arc<FontSystem> {
    let system = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
    system
        .register(face("NotoSans-Regular.ttf"), None)
        .expect("the Latin face registers");
    system
        .register(face("NotoSansArabic-Regular.ttf"), None)
        .expect("the Arabic face registers");
    system
}

/// A shaper over [`fonts`], forcing the base direction the given way.
pub fn shaper(controls: Controls) -> (Arc<FontSystem>, Shaper) {
    let system = fonts();
    let shaper = Shaper::with_controls(system.clone(), controls);
    (system, shaper)
}

/// A text style naming the shipped faces, Arabic first.
pub fn style() -> TextStyle {
    TextStyle {
        family: FontFamilyList::from_iter([
            FamilyName::Named(zgui_interned::Ident::new(ARABIC)),
            FamilyName::Named(zgui_interned::Ident::new(LATIN)),
            FamilyName::Generic(GenericFamily::SansSerif),
        ]),
        size: CssPx(16.0),
        ..TextStyle::initial()
    }
}

/// One paragraph of one style, with a map covering the whole of it verbatim.
pub struct Fixture {
    /// The generated text.
    pub text: String,
    /// Its map back to the source.
    pub map: TextMap,
    /// The single run.
    pub runs: Vec<StyledRun>,
    /// The paragraph's own properties.
    pub paragraph: ParagraphStyle,
}

impl Fixture {
    /// A paragraph holding `text` verbatim, laid out in `direction` and aligned to the start edge.
    pub fn new(text: &str, direction: Direction) -> Self {
        Self::sized(text, direction, 16.0)
    }

    /// The same, at a chosen font size in CSS pixels.
    pub fn sized(text: &str, direction: Direction, size: f32) -> Self {
        let mut map = TextMap::new();
        map.push(0..text.len(), 0, 0);
        Self {
            text: text.to_owned(),
            map,
            runs: vec![StyledRun {
                text: 0..text.len(),
                style: Arc::new(TextStyle {
                    size: CssPx(size),
                    ..style()
                }),
                brush: PaintSlot(0),
            }],
            paragraph: ParagraphStyle {
                direction,
                align: TextAlign::Start,
                ..ParagraphStyle::initial()
            },
        }
    }

    /// The content a shaper is handed.
    pub fn content(&self) -> ParagraphContent<'_> {
        ParagraphContent {
            text: &self.text,
            map: &self.map,
            runs: &self.runs,
            boxes: &[],
            paragraph: &self.paragraph,
            scale: 1.0,
        }
    }
}

/// The bidirectional fixture: a Latin filename first, then an Arabic body.
///
/// The first strong character is Latin, so automatic detection reads the paragraph as
/// left-to-right — which is exactly the case a forced right-to-left base direction has to override.
pub const BIDI: &str = "report.pdf ملف تقرير سنوي";

/// The mirror of [`BIDI`]: every strong character in it is Arabic.
///
/// Automatic detection reads it as right-to-left, so it is the fixture a forced *left-to-right*
/// base direction has to override. Asserting a left-to-right result over [`BIDI`] instead would
/// pass with no mark prefixed at all, since [`BIDI`] already detects as left-to-right.
pub const RTL_ONLY: &str = "ملف تقرير سنوي";
