//! Whether a run of words is on the screen, and where its letters landed.
//!
//! Every assertion here is made against the frame the device actually drew. Two readings are taken
//! and neither on its own is enough: a display list holding the glyphs is what separates text that
//! was drawn from text that was merely laid out, and pixels differing from the colour around them
//! is what separates text drawn where somebody can see it from text drawn under something else,
//! off the surface, or in the background's own colour.
//!
//! # Why the tiles, and not the pixels
//!
//! A glyph is rasterised once per face, size and glyph index and cached under that key, so two runs
//! of the same string in the same style read the same tiles in the same order — and two different
//! strings do not. That makes [`spelling`] an assertion about *which letters* were drawn, which a
//! photograph cannot be: a picture of a slide saying `Two` and a picture of one saying `One` differ
//! only in ways a fixture would have to recognise letters to tell apart.

use zgui::geom::{Device, DevicePx, Point, Rect};

use crate::desktop::census::Seen;
use crate::painted::stage::Stage;

/// How far a run of words got towards being on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reached {
    /// Nothing in the document says it. It never mounted, so nothing downstream ran.
    Unmounted,
    /// It is in the document and has no box, or a box of no size. It mounted and was never placed.
    Unplaced,
    /// It has a box and nothing was drawn inside it. It was placed and never painted.
    Unpainted,
    /// Its words are on the screen.
    Painted,
}

/// How far the thing saying `text` got, read off the last frame the device drew.
///
/// The *smallest* laid-out node whose whole text is `text` is the one asked about, and that matters
/// as much as either reading. Several nodes share one label — the text node, the panel it is on,
/// the positioner around that, the overlay band the positioner hangs off — and the band is the size
/// of the window. A reading taken from the outermost of them is a reading of the whole window, in
/// which one line of text is a fraction of a per cent of the pixels and every surface, painted or
/// not, looks unpainted.
pub fn reached(stage: &Stage, text: &str) -> Reached {
    let census = stage.census();
    let saying: Vec<&Seen> = census
        .nodes
        .iter()
        .filter(|node| node.text == text)
        .collect();
    if saying.is_empty() {
        return Reached::Unmounted;
    }
    let Some(seen) = saying
        .iter()
        .copied()
        .filter(|node| node.area() > 0.0)
        .min_by(|left, right| left.area().total_cmp(&right.area()))
    else {
        return Reached::Unplaced;
    };
    let rect = seen.rect.expect("a node with area has a box");
    if stage.glyphs_in(rect).is_empty() || ink(stage, rect) < 0.01 {
        return Reached::Unpainted;
    }
    Reached::Painted
}

/// What fraction of the pixels inside `rect` differ from the colour it is mostly made of.
///
/// The rectangle's own commonest colour rather than one named here, because a surface sits on
/// whatever is behind it and a fixture that named the background would be asserting against a style
/// sheet instead of against the picture.
pub fn ink(stage: &Stage, rect: Rect<DevicePx, Device>) -> f32 {
    let colours = stage.colours_in(rect);
    if colours.is_empty() {
        return 0.0;
    }
    let mut counts: rustc_hash::FxHashMap<(u8, u8, u8), u32> = rustc_hash::FxHashMap::default();
    for colour in &colours {
        *counts.entry(*colour).or_default() += 1;
    }
    let Some((background, _)) = counts.iter().max_by_key(|(_, count)| **count) else {
        return 0.0;
    };
    let background = *background;
    let differing = colours
        .iter()
        .filter(|colour| {
            (i32::from(colour.0) - i32::from(background.0)).abs() > 8
                || (i32::from(colour.1) - i32::from(background.1)).abs() > 8
                || (i32::from(colour.2) - i32::from(background.2)).abs() > 8
        })
        .count();
    differing as f32 / colours.len() as f32
}

/// Where the smallest laid-out thing saying `text` is.
///
/// # Panics
///
/// Panics when nothing says it, because a gesture aimed at the origin reports the same thing as a
/// control that does not answer.
pub fn aim(stage: &Stage, text: &str) -> Point<DevicePx, Device> {
    stage
        .census()
        .control(text)
        .and_then(|seen| seen.centre())
        .unwrap_or_else(|| panic!("nothing laid out says {text:?} to aim at"))
}

/// Asserts that the thing saying `text` is on the screen, naming the stage it stopped at.
pub fn assert_painted(stage: &Stage, text: &str) {
    let reached = reached(stage, text);
    assert_eq!(
        reached,
        Reached::Painted,
        "the surface saying {text:?} is not on the screen: it got as far as {reached:?}"
    );
}

/// Asserts that nothing on the screen says `text`.
pub fn assert_absent(stage: &Stage, text: &str) {
    let reached = reached(stage, text);
    assert_ne!(
        reached,
        Reached::Painted,
        "{text:?} is on the screen and nothing has opened it"
    );
}

/// Which letters the last frame drew inside `rect`, left to right, as the tiles they came from.
pub fn spelling(stage: &Stage, rect: Rect<DevicePx, Device>) -> Vec<(u32, u32)> {
    stage
        .glyphs_in(rect)
        .into_iter()
        .map(|glyph| glyph.tile)
        .collect()
}

/// Where inside `rect` the letters `word` were drawn, or nothing if they were not drawn there.
///
/// `word` is the spelling of a reference line rendered in the same style elsewhere on the page —
/// which is what makes this an assertion that *those letters* landed here, rather than that
/// something did. The answer is the left edge of the first of them, so two readings a step apart
/// say how far the thing holding them travelled.
pub fn found(stage: &Stage, rect: Rect<DevicePx, Device>, word: &[(u32, u32)]) -> Option<f32> {
    assert!(!word.is_empty(), "a reference line that drew no letters");
    let drawn = stage.glyphs_in(rect);
    let tiles: Vec<(u32, u32)> = drawn.iter().map(|glyph| glyph.tile).collect();
    tiles
        .windows(word.len())
        .position(|run| run == word)
        .map(|at| drawn[at].bounds.origin.x.0)
}
