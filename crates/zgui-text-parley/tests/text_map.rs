//! The generated-to-source offset map, over strings that actually stress it.

mod support;

use std::sync::Arc;

use zgui_scene::PaintSlot;
use zgui_text::{ParagraphContent, ParagraphShaper, StyledRun, TextMap};
use zgui_text_parley::Controls;
use zgui_text_style::{Direction, ParagraphStyle, TextAlign};

/// A deterministic generator, so that a failure is reproducible and a run is comparable across
/// machines.
struct Rng(u64);

impl Rng {
    /// The next value.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A value below `bound`.
    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// The pieces a source string is built from: ordinary words, collapsible white space in several
/// forms, a soft hyphen, an inner isolate pair, and Arabic.
const PIECES: [&str; 14] = [
    "alpha",
    "beta",
    " ",
    "  ",
    "\t",
    "\n",
    "\u{00ad}",
    "ملف",
    "تقرير",
    "gamma",
    " \n\t ",
    "سنوي",
    "\u{2067}",
    "\u{2069}",
];

/// Whether a character is one an inline formatting context emits without the source having written
/// it as text — the isolate controls a `unicode-bidi` value is expressed with.
fn is_control(character: char) -> bool {
    ('\u{2066}'..='\u{2069}').contains(&character)
}

/// One generated string and the map back to the source it was collapsed from.
struct Generated {
    /// The collapsed text.
    text: String,
    /// Where each surviving stretch came from.
    map: TextMap,
    /// How many source bytes were dropped by collapsing.
    dropped: usize,
}

/// Collapses white space the way an inline formatting context does, recording the map as it goes.
///
/// This is the caller's half of the arrangement — the engine is driven with collapsing switched
/// off — and it is written out here because the map can only be built at the moment the
/// correspondence is known.
fn generate(source: &str) -> Generated {
    let mut out = Generated {
        text: String::new(),
        map: TextMap::new(),
        dropped: 0,
    };
    let mut pending: Option<usize> = None;
    let mut seen = false;
    for (offset, character) in source.char_indices() {
        if character.is_whitespace() {
            if pending.is_none() {
                pending = Some(offset);
            } else {
                out.dropped += character.len_utf8();
            }
            continue;
        }
        if let Some(at) = pending.take() {
            if seen {
                let start = out.text.len();
                out.text.push(' ');
                out.map.push(start..out.text.len(), 0, at);
            } else {
                out.dropped += 1;
            }
        }
        let start = out.text.len();
        out.text.push(character);
        if is_control(character) {
            // An isolate control belongs to no source position, exactly as the paragraph's own
            // directional prefix does, so it is emitted and not mapped.
            out.dropped += character.len_utf8();
        } else {
            out.map.push(start..out.text.len(), 0, offset);
        }
        seen = true;
    }
    if pending.is_some() {
        out.dropped += 1;
    }
    out
}

/// Every generated offset that maps to a source position maps forward again to itself.
///
/// The guards below are what keep this from passing while testing nothing. A round trip over text
/// that collapsed nothing, contains no control character and carries no directional prefix is a
/// round trip over the identity function; the counters assert that a substantial share of the
/// cases exercised each of the three ways an offset can fail to have a source.
#[test]
fn text_map_round_trips() {
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let paragraph = ParagraphStyle {
        direction: Direction::RightToLeft,
        align: TextAlign::Start,
        ..ParagraphStyle::initial()
    };
    let style = Arc::new(support::style());

    let mut rng = Rng(0x5eed_1234_9abc_def0);
    let (mut mapped, mut unmapped, mut with_collapse) = (0usize, 0usize, 0usize);
    let (mut cases, mut dropped_from_source) = (0usize, 0usize);

    for _ in 0..1_000 {
        let mut source = String::new();
        for _ in 0..1 + rng.below(6) {
            source.push_str(PIECES[rng.below(PIECES.len())]);
        }
        let generated = generate(&source);
        // A case whose every character is a control has no source content at all, so there is
        // nothing for an offset in it to snap to; it says nothing about the map either way.
        if generated.text.is_empty() || generated.map.is_empty() {
            continue;
        }
        cases += 1;
        if generated.dropped > 0 {
            with_collapse += 1;
        }
        let runs = [StyledRun {
            text: 0..generated.text.len(),
            style: style.clone(),
            brush: PaintSlot(0),
        }];
        let content = ParagraphContent {
            text: &generated.text,
            map: &generated.map,
            runs: &runs,
            boxes: &[],
            paragraph: &paragraph,
            scale: 1.0,
        };
        let shaped = shaper.shape(&content);
        let map = shaped.map();

        // The other direction: a source byte that collapsing dropped has no generated offset, and
        // saying so is what a caret placed on it has to be able to rely on.
        for (offset, _) in source.char_indices() {
            if map
                .to_generated(zgui_text::SourcePos { run: 0, offset })
                .is_none()
            {
                dropped_from_source += 1;
            }
        }

        for offset in 0..shaped.text().len() {
            if !shaped.text().is_char_boundary(offset) {
                continue;
            }
            match map.to_source(offset) {
                Some(position) => {
                    mapped += 1;
                    assert_eq!(
                        map.to_generated(position),
                        Some(offset),
                        "{:?} at {offset} did not round trip",
                        shaped.text()
                    );
                }
                None => {
                    unmapped += 1;
                    assert!(
                        map.to_source_snapped(offset).is_some(),
                        "an offset with no source must still snap to one"
                    );
                }
            }
        }
    }

    assert!(
        cases > 800,
        "only {cases} of the thousand cases had any text"
    );
    assert!(mapped > 1_000, "only {mapped} offsets had a source at all");
    assert!(
        unmapped > 200,
        "{unmapped} offsets had no source over {cases} cases; the isolate controls the fixtures \
         write are the only characters the source does not account for, and there have to be some \
         — the directional prefix is not among them, because no offset reported here counts it"
    );
    assert!(
        dropped_from_source > 200,
        "only {dropped_from_source} source bytes were dropped by collapsing, so the reverse \
         direction was barely exercised"
    );
    assert!(
        with_collapse > 200,
        "only {with_collapse} cases collapsed any white space, so the interesting half was rare"
    );
}

/// A round trip over text with nothing to collapse and no prefix is the identity, and says so.
///
/// Kept beside the property test as the control it is measured against: this is what a
/// round-trip assertion looks like when it is exercising nothing.
#[test]
fn a_round_trip_over_untouched_text_exercises_nothing() {
    let (_fonts, mut shaper) = support::shaper(Controls::Verbatim);
    let fixture = support::Fixture::new("alphabeta", Direction::LeftToRight);
    let shaped = shaper.shape(&fixture.content());
    let map = shaped.map();
    let unmapped = (0..shaped.text().len())
        .filter(|offset| map.to_source(*offset).is_none())
        .count();
    assert_eq!(
        unmapped, 0,
        "every offset has a source, so a round trip here proves nothing about the map"
    );
}

/// The engine collapses nothing: the string it is handed is the string it shapes.
///
/// This is the half of the arrangement that makes the map above possible at all. The engine's own
/// collapsing is ASCII-only and reports no correspondence back to the source, so it could serve
/// neither the collapsing decision nor the map; the caller collapses, and what it hands over is
/// taken verbatim.
#[test]
fn the_engine_collapses_nothing() {
    let (_fonts, mut shaper) = support::shaper(Controls::Verbatim);
    let uncollapsed = "two  spaces\tand\na tab";
    let fixture = support::Fixture::new(uncollapsed, Direction::LeftToRight);
    let shaped = shaper.shape(&fixture.content());
    assert_eq!(
        shaped.text(),
        uncollapsed,
        "every space, tab and newline survives into the shaped string"
    );
    for offset in 0..uncollapsed.len() {
        assert_eq!(
            shaped.map().to_source(offset).map(|at| at.offset),
            Some(offset),
            "and each one still maps to where it came from"
        );
    }
}
