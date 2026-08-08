//! Building the one string a shaper is handed, and the way back from it.
//!
//! # What white space does on the way through
//!
//! CSS does not shape the text a document holds. Under `white-space-collapse: collapse` every run
//! of spaces, tabs and newlines becomes a single space, a run at the very start of the context or
//! immediately after a forced break disappears, and so does one at the very end. Under `preserve`
//! nothing is touched except a tab, which stands for a jump to the next tab stop rather than for a
//! character of its own.
//!
//! Every one of those is a change to the byte offsets, which is why the map is built as the string
//! is: this is the only point at which the correspondence is known. The synthetic space a collapsed
//! run becomes is mapped to the run's *first* byte, so a caret placed on it lands where the white
//! space began rather than nowhere.

use core::ops::Range;

use zgui_css::ComputedStyle;
use zgui_css::values::text::TabSize;
use zgui_dom::side::BoxKey;
use zgui_text::{Brush, StyledRun, TextMap, Transformer};
use zgui_text_style::{LengthPercent, ParagraphStyle, TextPaint};

use crate::inline::content::collect::Piece;
use crate::inline::content::styles::{RunStyle, TextStyles};
use crate::inline::content::{Generated, Item, Role};
use crate::tree::store::LayoutStore;

/// The pieces, generated into a string with a map back to them.
///
/// `claim` is asked for a brush slot once per distinct style, because a brush is an index into a
/// table this has no way to reach.
pub(crate) fn build(
    store: &LayoutStore,
    root: BoxKey,
    pieces: &[Piece],
    styles: &mut TextStyles,
    claim: &mut impl FnMut(&TextPaint) -> Brush,
    scale: f32,
) -> Generated {
    let root_style = styles.get(&store.node(root).style);
    let root_brush = styles.brush(&store.node(root).style, &root_style.paint, &mut *claim);
    let mut builder = Builder::default();

    for piece in pieces {
        match *piece {
            Piece::Text(key) => {
                let node = store.node(key);
                let style = styles.get(&node.style);
                let brush = styles.brush(&node.style, &style.paint, &mut *claim);
                let tab = tab_spaces(&node.style);
                let text = node.text.as_deref().unwrap_or_default();
                builder.text_run(key, text, &style, brush, tab);
            }
            Piece::Atomic(key) => builder.item(Role::Atomic(key)),
            Piece::Enter(key) => builder.item(Role::StartEdge(key)),
            Piece::Leave(key) => builder.item(Role::EndEdge(key)),
        }
    }

    let runs = builder.finish();
    let sources = builder.runs.iter().map(|run| run.source).collect();
    Generated {
        key: std::sync::OnceLock::new(),
        text: builder.text,
        map: builder.map,
        runs,
        sources,
        items: builder.items,
        paragraph: scaled_paragraph(root_style.paragraph, scale),
        root: root_style.text,
        root_brush,
    }
}

/// The paragraph style, with its absolute indent already in the units layout works in.
///
/// Everything a shaper reads out of a *run* style is in CSS pixels and is scaled by the shaper
/// itself. The indent is not: it is applied to the broken lines directly, in whatever units they
/// came out in. So it is converted here, once, rather than being the one length in the pipeline
/// that arrives in the wrong space.
fn scaled_paragraph(mut paragraph: ParagraphStyle, scale: f32) -> ParagraphStyle {
    paragraph.indent.length = LengthPercent {
        length: zgui_geom::CssPx(paragraph.indent.length.length.0 * scale),
        percent: paragraph.indent.length.percent,
    };
    paragraph
}

/// How many spaces one preserved tab stands for.
///
/// The count form is exact: `tab-size: 4` is four space advances. The length form asks for a tab
/// stop at a distance no character count can express, because it depends on the shaped advance of
/// the line so far; it is not honoured, and a tab under it advances by one space.
fn tab_spaces(style: &ComputedStyle) -> usize {
    match style.get_inherited_text().tab_size {
        TabSize::Number(count) => (count.0.max(0.0).round() as usize).max(1),
        TabSize::Length(_) => 1,
    }
}

/// The state one generation carries.
struct Builder {
    /// The string so far.
    text: String,
    /// The map so far.
    map: TextMap,
    /// One entry per text run: its style, its brush, its source box and the end of the last stretch
    /// attributed to it.
    runs: Vec<Pending>,
    /// The inline boxes so far.
    items: Vec<Item>,
    /// A collapsible white-space run that has been seen and not yet emitted, and where in the
    /// source it started.
    pending: Option<(usize, usize)>,
    /// Whether nothing that can carry a space has been emitted since the last forced break.
    at_start: bool,
    /// The next inline box identifier.
    next_id: u64,
    /// The `text-transform` state, carried across runs.
    ///
    /// Across runs, because an inline box boundary leaves a word running: a word split by a
    /// `<span>` is one word, so `capitalize` capitalises it once, and a Greek sigma at the end of
    /// one run is decided by the letter the next run starts with. Each run reconfigures the
    /// transformer to its own property and the state carries through, which is what makes both of
    /// those come out right.
    transform: Option<Transformer>,
}

/// One run under construction.
struct Pending {
    /// Its style.
    style: std::sync::Arc<zgui_text_style::TextStyle>,
    /// The box whose characters it is, which is the way back from an offset in the generated
    /// string to the text node a caret or a selection has to be expressed against.
    source: BoxKey,
    /// Its brush.
    brush: Brush,
    /// The end of the last stretch attributed to it.
    end: usize,
}

impl Builder {
    /// Appends one run of text.
    fn text_run(&mut self, source: BoxKey, text: &str, style: &RunStyle, brush: Brush, tab: usize) {
        let index = self.runs.len();
        self.runs.push(Pending {
            style: style.text.clone(),
            source,
            brush,
            end: self.text.len(),
        });
        self.reconfigure_transform(style);

        let collapse = style.text.white_space.collapses_spaces();
        let newlines = style.text.white_space.preserves_newlines();
        for (offset, character) in text.char_indices() {
            let width = character.len_utf8();
            match character {
                '\n' if newlines => {
                    self.pending = None;
                    self.emit(index, offset..offset + width, "\n");
                    self.interrupt_transform();
                    self.at_start = true;
                }
                '\t' if !collapse => {
                    self.flush();
                    // The spaces stand for one tab, so they are kept away from the transform:
                    // `full-width` maps `U+0020` alone, and turning the expansion into ideographic
                    // spaces would change how far the tab advances.
                    let spaces = " ".repeat(tab);
                    self.emit(index, offset..offset + width, &spaces);
                    self.interrupt_transform();
                    self.at_start = false;
                }
                _ if collapse && is_collapsible(character) => {
                    self.pending.get_or_insert((index, offset));
                }
                '\r' => {}
                _ => {
                    self.flush();
                    self.emit_char(index, offset..offset + width, character, !collapse);
                    self.at_start = false;
                }
            }
        }
    }

    /// Puts this run's `text-transform` in force, keeping the state the previous runs built up.
    ///
    /// The state is what an inline box boundary must not disturb: a word split by a `<span>` is one
    /// word, so `capitalize` capitalises it once, and a Greek sigma at the end of one run is decided
    /// by the letter the next run begins with. Both cross the boundary, so the transformer does.
    ///
    /// Nothing is created until the first run that actually asks for a transform, which is nearly
    /// every document's whole answer: a paragraph with no `text-transform` anywhere never builds one
    /// and every character takes the path it took before. The first that does resumes from the last
    /// character already generated, so the word it may be in the middle of is not restarted.
    fn reconfigure_transform(&mut self, style: &RunStyle) {
        let transform = style.text.transform;
        let language = style.text.language.as_ref().map(|tag| tag.as_str());
        match self.transform.as_mut() {
            Some(transformer) => transformer.reconfigure(transform, language),
            None if !transform.is_none() => {
                let mut transformer = Transformer::new(transform, language);
                transformer.resume_after(self.text.chars().next_back());
                self.transform = Some(transformer);
            }
            None => {}
        }
    }

    /// Tells the transform that something which is not a letter came between two characters.
    fn interrupt_transform(&mut self) {
        if let Some(transformer) = self.transform.as_mut() {
            transformer.interrupt();
        }
    }

    /// Records one inline box at the current offset.
    fn item(&mut self, role: Role) {
        // White space either side of something opaque survives as one space each side, exactly as
        // it does either side of a word.
        self.flush();
        if role.is_atomic() {
            // An image is not a letter, so the word ends at it and the sigma before it is final.
            self.interrupt_transform();
        }
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Item {
            id,
            role,
            offset: self.text.len(),
        });
        if role.is_atomic() {
            self.at_start = false;
        }
    }

    /// Emits the one space a pending collapsible run stands for, if there is one to emit.
    ///
    /// The space is attributed to the run the white space was written in, which is what makes the
    /// space between two differently styled words take the first one's style.
    fn flush(&mut self) {
        let Some((run, offset)) = self.pending.take() else {
            return;
        };
        // A run of white space at the very start of the context, or straight after a forced break,
        // is not a space at all.
        if self.at_start {
            return;
        }
        // The one space a collapsed run becomes is not white space the style preserved, so
        // `full-width` leaves it as a space; it still passes through the transform, because it is
        // what ends the word before it.
        self.emit_char(run, offset..offset + 1, ' ', false);
        self.at_start = false;
    }

    /// Appends `generated` to the string, attributing it to `run`'s bytes at `source`.
    fn emit(&mut self, run: usize, source: Range<usize>, generated: &str) {
        let start = self.text.len();
        self.text.push_str(generated);
        self.record(run, source, start, true);
    }

    /// Appends one character, through the `text-transform` in force if there is one.
    ///
    /// `preserved` says whether the character is white space the style keeps, which is the one
    /// thing `full-width` distinguishes.
    fn emit_char(&mut self, run: usize, source: Range<usize>, character: char, preserved: bool) {
        let start = self.text.len();
        let verbatim = match self.transform.as_mut() {
            Some(transformer) => {
                transformer.push(character, preserved, &mut self.text);
                let mut buffer = [0_u8; 4];
                self.text[start..] == *character.encode_utf8(&mut buffer)
            }
            None => {
                self.text.push(character);
                true
            }
        };
        self.record(run, source, start, verbatim);
    }

    /// Records what was appended from `start` onwards as coming from `run`'s bytes at `source`.
    ///
    /// `verbatim` says whether those bytes are the source bytes themselves. Length alone leaves it
    /// open, and the difference is a real defect: `ß` upper-cases to `SS`, two bytes standing in for
    /// two bytes, so treating equal lengths as a copy would put a caret between the two `S`s and
    /// report it as standing inside the `ß`.
    fn record(&mut self, run: usize, source: Range<usize>, start: usize, verbatim: bool) {
        let generated = self.text.len() - start;
        if verbatim && generated == source.len() {
            self.map.push(start..self.text.len(), run, source.start);
        } else {
            // A tab that stood for several spaces, or a letter the transform replaced. Every
            // generated byte belongs to the one source position, so each is a stretch of its own. A
            // caret therefore lands either side of the `SS` and never inside it, which is right: the
            // source has one character there.
            for offset in 0..generated {
                self.map
                    .push(start + offset..start + offset + 1, run, source.start);
            }
        }
        if let Some(entry) = self.runs.get_mut(run) {
            entry.end = self.text.len();
        }
    }

    /// The styled ranges, tiling the string with no gaps.
    ///
    /// A run's range runs from wherever the previous one stopped to the end of the last stretch
    /// attributed to it. That is not the same as "where it started emitting": a collapsed space is
    /// emitted while the *next* run is being written and belongs to the previous one, and a run
    /// whose every character collapsed away is empty rather than absent.
    fn finish(&self) -> Vec<StyledRun> {
        let mut runs = Vec::with_capacity(self.runs.len());
        let mut next = 0;
        for (index, pending) in self.runs.iter().enumerate() {
            let end = if index + 1 == self.runs.len() {
                pending.end.max(next).max(self.text.len())
            } else {
                pending.end.max(next)
            };
            runs.push(StyledRun {
                text: next..end,
                style: pending.style.clone(),
                brush: pending.brush,
            });
            next = end;
        }
        runs
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            text: String::new(),
            map: TextMap::new(),
            runs: Vec::new(),
            items: Vec::new(),
            pending: None,
            // Nothing has been emitted, so a run of white space starting here is one CSS drops
            // rather than one it collapses to a space.
            at_start: true,
            next_id: 0,
            transform: None,
        }
    }
}

/// Whether a character is white space that collapses.
fn is_collapsible(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r' | '\u{000c}')
}
