//! The break a context was kept at, and the step that puts each shaped form back on it.
//!
//! A shaped paragraph holds one break at a time, and paint reads its glyphs from that break. The
//! kept pass leaves the form on the lines it resolved to. A size probe that is not answered from a
//! remembered break moves the form to the probe's width, and a later kept layout served from the
//! engine's cache runs no pass that moves it back. So every pass that breaks notes its box, and the
//! end of the layout pass breaks each noted box's form at its kept request again where the form
//! moved.
//!
//! Contexts with equal content share one form, so they have to keep equal lines. A context that
//! keeps other lines gets a copy of the form; see `restore`.

use std::sync::Arc;

use rustc_hash::FxHashMap;
use zgui_dom::side::BoxKey;
use zgui_geom::CssPx;
use zgui_profile::{Counter, counter};
use zgui_text::{
    BreakRequest, InlineBoxGeometry, LineBand, LineBands, ParagraphContent, ParagraphKey,
};
use zgui_text_style::{BreakingKey, Digest, TextAlign, TextAlignLast};

use crate::fragment::ParagraphId;
use crate::inline::content::Generated;
use crate::inline::resolved::InlineResolution;
use crate::measure::MeasureContent;
use crate::tree::store::LayoutStore;

/// The request a context's lines were broken under.
///
/// Holds no allocation of its own for a context with no atomic inline and no float beside it,
/// which is nearly every context.
#[derive(Clone, Debug)]
pub struct KeptBreak {
    /// The flattened content: the runs and the paragraph style.
    pub(crate) generated: Arc<Generated>,
    /// The atomic inlines, at the geometry of the last break.
    pub(crate) boxes: Vec<InlineBoxGeometry>,
    /// The width the lines were broken into.
    pub(crate) max_advance: Option<CssPx>,
    /// The width a percentage indent was resolved against.
    pub(crate) indent_basis: Option<CssPx>,
    /// The width of each line beside a float.
    pub(crate) bands: Vec<LineBand>,
    /// The key the form reflected right after the kept break, which saves hashing the request
    /// again. `None` for lines a probe left, whose break may have been recalled.
    pub(crate) key: Option<BreakingKey>,
}

impl KeptBreak {
    /// The request, as the kept pass made it.
    pub(crate) fn request(&self) -> BreakRequest<'_> {
        BreakRequest {
            runs: &self.generated.runs,
            boxes: &self.boxes,
            paragraph: &self.generated.paragraph,
            max_advance: self.max_advance,
            indent_basis: self.indent_basis,
            bands: LineBands::new(&self.bands),
            probe: false,
        }
    }

    /// The key of the request, as the kept pass made it.
    pub(crate) fn key(&self) -> BreakingKey {
        self.key.unwrap_or_else(|| self.request().key())
    }
}

/// What paint reads from a form broken at one context's kept request.
///
/// Paint places the glyphs of each line against the line box, and the line box holds the
/// alignment offset. So for most paragraphs the glyphs depend only on which text each line holds,
/// and two contexts at different widths with equal lines can share one form. A justified line
/// also spaces its glyphs by the width, and an atomic inline, an inline box edge or a float moves
/// glyphs in a way the line ranges do not record. Such a paragraph is keyed by its whole kept
/// request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct LinesKey(u64);

impl LinesKey {
    /// The value a paragraph record holds before a pass records its lines.
    pub(crate) const NONE: Self = Self(0);

    /// The key held as `bits`, as [`LinesKey::bits`] gave it.
    pub(crate) const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    /// The bits of the key, for a record that holds them as a plain number.
    pub(crate) const fn bits(self) -> u64 {
        self.0
    }

    /// The key of the lines `resolution` keeps.
    pub(crate) fn of(resolution: &InlineResolution) -> Self {
        let kept = &resolution.kept;
        let paragraph = &kept.generated.paragraph;
        let mut digest = Digest::new();
        let plain = kept.boxes.is_empty()
            && kept.bands.is_empty()
            && kept.generated.items.is_empty()
            && paragraph.align != TextAlign::Justify
            && paragraph.align_last != TextAlignLast::Justify;
        if plain {
            digest.push_tag(0);
            digest.push_length(kept.request().indent());
            digest.push(resolution.lines.len());
            for line in &resolution.lines {
                digest.push(line.text.start);
                digest.push(line.text.end);
            }
        } else {
            digest.push_tag(1);
            digest.push(kept.key().0);
        }
        Self(digest.finish())
    }
}

/// The buffers [`restore`] uses, kept on the store across passes.
#[derive(Debug, Default)]
pub(crate) struct Scratch {
    /// The tally of each shared paragraph that a noted context names.
    tallies: FxHashMap<ParagraphId, Tally>,
    /// The lines each noted context keeps, in the order of the noted list.
    lines: Vec<LinesKey>,
}

/// How the noted contexts on one shared paragraph stand against its recorded lines.
#[derive(Clone, Copy, Debug, Default)]
struct Tally {
    /// How many contexts used the paragraph when the pass ended.
    users: u32,
    /// How many noted contexts name the paragraph.
    noted: u32,
    /// How many of them keep the recorded lines.
    matching: u32,
    /// Whether one of them recorded its own lines for the paragraph in this pass.
    reclaimed: bool,
    /// Whether the form holds lines that every user keeps.
    settled: bool,
}

/// Separates each shared paragraph whose contexts keep different lines, and breaks each noted
/// context's form at its kept request again where a pass moved it.
///
/// Called once at the end of a layout pass, with the device scale of the pass. A noted box costs
/// one lookup of its form's break. A noted box on a shared paragraph also costs one hash of its
/// line ranges and a tally entry. A paragraph with one user records that user, and the hash of its
/// lines is computed only when a second user asks for it.
///
/// # Separation
///
/// Contexts with equal content share one shaped paragraph, and the paragraph holds the lines of
/// one break. Paint reads the glyphs of each line from it, so every context that shares it has to
/// keep the same lines ([`LinesKey`]). The paragraph's record holds them. A context that comes
/// out of this pass with other lines moves to a copy of the paragraph
/// ([`ParagraphKey::variant`]), which the contexts that keep those lines share. Later passes
/// break that copy, see `shape_own` in [`measure`](crate::inline::measure).
///
/// When each user of a paragraph was laid out in this pass and none of them kept the recorded
/// lines, the first one records its lines for the paragraph. A list of equal rows that all change
/// width thus keeps one shaped paragraph.
///
/// Each shared paragraph then needs one break whose lines its users keep. The first noted user
/// whose break the form reflects settles it, else the first noted user breaks it. When every
/// noted user left the paragraph, one remaining user is found by a walk of the tree.
pub(crate) fn restore<C: MeasureContent>(store: &mut LayoutStore, content: &mut C, scale: f32) {
    let mut noted = core::mem::take(&mut store.noted);
    if noted.is_empty() {
        store.noted = noted;
        return;
    }
    let mut scratch = core::mem::take(&mut store.kept_scratch);
    for &(key, _) in &noted {
        if let Some(state) = store.state_opt_mut(key) {
            state.noted = false;
        }
    }
    tally(store, &noted, &mut scratch);
    let Scratch { tallies, lines } = &mut scratch;

    for (&(key, base), &kept) in noted.iter().zip(lines.iter()) {
        let Some(resolution) = store.inline_resolution(key) else {
            continue;
        };
        let id = resolution.paragraph;
        if store.paragraph_users(id) <= 1 {
            store.claim_paragraph_for(id, key);
            continue;
        }
        let kept = known(kept, resolution);
        if !shapes(store, resolution, base) {
            continue;
        }
        let claimed = store.paragraph_lines(id);
        if claimed == Some(kept) {
            continue;
        }
        let tally = tallies
            .get_mut(&id)
            .expect("a shared paragraph has a tally");
        // Nobody keeps the recorded lines when every user was noted and none matched them.
        let unkept = claimed.is_none() || (tally.matching == 0 && tally.noted == tally.users);
        if unkept && !tally.reclaimed {
            tally.reclaimed = true;
            store.claim_paragraph(id, kept);
            continue;
        }
        match separate(store, content, key, base, kept, scale) {
            // The copy may be shared too. The context recorded its lines for it, and its users
            // settle it below, as they do the others.
            Some(copy) => tallies.entry(copy).or_default().reclaimed = true,
            // A measurer with no copies: the users share the form as they did before.
            None => tally.settled = true,
        }
    }

    // A shared form whose break is the kept break of a user with the recorded lines is settled.
    for (&(key, _), &kept) in noted.iter().zip(lines.iter()) {
        let Some(resolution) = store.inline_resolution(key) else {
            continue;
        };
        let id = resolution.paragraph;
        if let Some(tally) = tallies.get_mut(&id)
            && !tally.settled
            && store.paragraph_lines(id) == Some(known(kept, resolution))
            && content.breaks_reflect(resolution.key) == Some(resolution.kept.key())
        {
            tally.settled = true;
        }
    }

    for (&(key, _), &kept) in noted.iter().zip(lines.iter()) {
        let Some(resolution) = store.inline_resolution(key) else {
            continue;
        };
        let id = resolution.paragraph;
        let tally = tallies.get_mut(&id);
        let shared = store.paragraph_users(id) > 1
            && store.paragraph_lines(id) == Some(known(kept, resolution));
        match tally {
            Some(tally) if shared && tally.settled => continue,
            Some(tally) => tally.settled |= shared || store.paragraph_users(id) <= 1,
            None => {}
        }
        if restore_one(resolution, content) {
            tracing::debug!(target: "zgui::layout", box_ = key.index(), "text form restored");
        }
    }

    for (&id, tally) in tallies.iter() {
        if tally.settled || store.paragraph_users(id) == 0 {
            continue;
        }
        if let Some(user) = find_user(store, id)
            && let Some(resolution) = store.inline_resolution(user)
        {
            restore_one(resolution, content);
        }
    }

    debug_assert!(store.noted.is_empty(), "nothing notes during the restore");
    // Kept for the next pass only while small. A cold pass notes every context, and buffers kept
    // at that size would cost memory per row for the life of the document.
    if noted.capacity() <= SCRATCH_KEEP {
        noted.clear();
        store.noted = noted;
    }
    if lines.capacity() <= SCRATCH_KEEP && tallies.capacity() <= SCRATCH_KEEP {
        tallies.clear();
        lines.clear();
        store.kept_scratch = scratch;
    }
}

/// The most noted contexts whose buffers [`restore`] keeps for the next pass.
const SCRATCH_KEEP: usize = 256;

/// Fills `scratch` with the lines of each noted context, and the tally of each paragraph that a
/// noted context shares with other contexts.
fn tally(store: &LayoutStore, noted: &[(BoxKey, ParagraphKey)], scratch: &mut Scratch) {
    for &(key, base) in noted {
        let Some(resolution) = store.inline_resolution(key) else {
            // Never read: the loops over the noted list skip a box with no resolution.
            scratch.lines.push(LinesKey::NONE);
            continue;
        };
        let id = resolution.paragraph;
        let users = store.paragraph_users(id);
        if users <= 1 {
            // A paragraph with one user records the user, not the key of its lines.
            scratch.lines.push(LinesKey::NONE);
            continue;
        }
        let kept = LinesKey::of(resolution);
        scratch.lines.push(kept);
        if !shapes(store, resolution, base) {
            continue;
        }
        let tally = scratch.tallies.entry(id).or_insert_with(|| Tally {
            users,
            ..Tally::default()
        });
        tally.noted += 1;
        if store.paragraph_lines(id) == Some(kept) {
            tally.matching += 1;
        }
    }
}

/// The key of the lines `resolution` keeps: `kept` as [`tally`] filled it, computed when
/// [`tally`] left it out because the paragraph had one user then.
fn known(kept: LinesKey, resolution: &InlineResolution) -> LinesKey {
    if kept == LinesKey::NONE {
        LinesKey::of(resolution)
    } else {
        kept
    }
}

/// Whether the paragraph `resolution` names holds the content `base` was hashed from: `base`
/// itself, or a copy of it.
fn shapes(store: &LayoutStore, resolution: &InlineResolution, base: ParagraphKey) -> bool {
    resolution.key == base || store.variant_base(resolution.key) == Some(base)
}

/// Moves the context `key` from a shared paragraph to the copy of `base` for the lines it keeps,
/// broken at its kept request.
///
/// Contexts that keep equal lines share one copy, under the key `base` and `lines` give. When that
/// copy is the paragraph the context leaves, or other users of it keep other lines now, the
/// context takes a copy of its own.
///
/// Returns the copy. A measurer that holds no copies leaves the context where it is.
fn separate<C: MeasureContent>(
    store: &mut LayoutStore,
    content: &mut C,
    key: BoxKey,
    base: ParagraphKey,
    lines: LinesKey,
    scale: f32,
) -> Option<ParagraphId> {
    let resolution = store.inline_resolution(key)?;
    let old = resolution.paragraph;
    let mut variant = base.variant(lines);
    if let Some(held) = store.paragraph_id(variant)
        && (held == old
            || (store.paragraph_users(held) > 0 && store.paragraph_lines(held) != Some(lines)))
    {
        variant = base.variant((lines, key));
    }
    let kept = &resolution.kept;
    let generated = &kept.generated;
    let paragraph = ParagraphContent {
        text: &generated.text,
        map: &generated.map,
        runs: &generated.runs,
        boxes: &kept.boxes,
        paragraph: &generated.paragraph,
        scale,
    };
    content.shape_variant(base, variant, &paragraph)?;
    content.break_lines(variant, &kept.request());
    let new = store.intern_paragraph(variant);
    store.note_variant(variant, base);
    store.retain_paragraph(new);
    store.release_paragraph(old);
    store.claim_paragraph(new, lines);
    let resolution = store
        .state_mut(key)
        .inline
        .as_deref_mut()
        .expect("the resolution read above");
    resolution.key = variant;
    resolution.paragraph = new;
    counter::bump(Counter::TextFormsSeparated);
    Some(new)
}

/// A context whose resolution names the paragraph `id`.
///
/// Walks the box tree. This happens only in a pass where each noted user of a shared paragraph
/// left it and other users stayed.
fn find_user(store: &LayoutStore, id: ParagraphId) -> Option<BoxKey> {
    let mut stack = vec![store.root()?];
    while let Some(key) = stack.pop() {
        if store
            .inline_resolution(key)
            .is_some_and(|resolution| resolution.paragraph == id)
        {
            return Some(key);
        }
        stack.extend(store.node(key).children.iter().copied());
    }
    None
}

/// Breaks one context's form at its kept request, if the form reflects another break.
///
/// Returns whether it did.
fn restore_one<C: MeasureContent>(resolution: &InlineResolution, content: &mut C) -> bool {
    match content.breaks_reflect(resolution.key) {
        Some(reflected) if reflected != resolution.kept.key() => {
            content.break_lines(resolution.key, &resolution.kept.request());
            counter::bump(Counter::TextFormsRestored);
            true
        }
        _ => false,
    }
}

/// Every context whose form reflects lines other than its kept ones.
///
/// A form reflects a context's lines when its break is the context's kept break, or the kept
/// break of another user of the same paragraph that keeps equal lines. Empty after a layout pass.
/// The check runs under `ZGUI_INVARIANTS`, at the end of every pass.
pub(crate) fn mismatched<C: MeasureContent>(store: &LayoutStore, content: &C) -> Vec<BoxKey> {
    let mut out = Vec::new();
    let Some(root) = store.root() else {
        return out;
    };
    let mut contexts = Vec::new();
    let mut stack = vec![root];
    while let Some(key) = stack.pop() {
        if store.inline_resolution(key).is_some() {
            contexts.push(key);
        }
        stack.extend(store.node(key).children.iter().copied());
    }
    // The kept break of each context that keeps the recorded lines of its paragraph.
    let mut kept: rustc_hash::FxHashSet<(ParagraphId, BreakingKey)> = Default::default();
    for &key in &contexts {
        let resolution = store.inline_resolution(key).expect("collected above");
        if store.paragraph_lines(resolution.paragraph) == Some(LinesKey::of(resolution)) {
            kept.insert((resolution.paragraph, resolution.kept.key()));
        }
    }
    for key in contexts {
        let resolution = store.inline_resolution(key).expect("collected above");
        let Some(reflected) = content.breaks_reflect(resolution.key) else {
            continue;
        };
        if reflected == resolution.kept.key() {
            continue;
        }
        let id = resolution.paragraph;
        let shares = store.paragraph_users(id) > 1
            && store.paragraph_lines(id) == Some(LinesKey::of(resolution))
            && kept.contains(&(id, reflected));
        if !shares {
            out.push(key);
        }
    }
    out
}

/// A description of each box and the chain of boxes above it, for the invariant's message.
pub(crate) fn describe(store: &LayoutStore, keys: &[BoxKey]) -> String {
    let mut out = String::new();
    for &key in keys {
        out.push_str(&format!("\n  box {}:", key.index()));
        let mut at = Some(key);
        while let Some(current) = at {
            let node = store.node(current);
            out.push_str(&format!(" <- {}:{:?}", current.index(), node.fc));
            at = node.parent;
        }
    }
    out
}
