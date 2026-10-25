//! A break that keeps the lines of the pass before it reports what a fresh break reports.
//!
//! A breaking pass keeps the previous line boxes and box placements when its lines are equal to
//! them. The proof is here: for random paragraphs broken at a random sequence of widths, every
//! answer equals, bit for bit, a reading of the engine's laid-out form made in this file. It also
//! equals the reading of a paragraph shaped fresh and broken once at the same request, and so do
//! the glyphs and clusters of the form.

mod support;

use std::sync::{Arc, MutexGuard, OnceLock};

use proptest::prelude::*;
use smallvec::SmallVec;
use zgui_geom::{CssPx, Point, Size};
use zgui_profile::counter::non_vacuity::{Scenario, assert_non_vacuous};
use zgui_profile::{Counter, counter};
use zgui_scene::PaintSlot;
use zgui_text::{
    BreakRequest, BrokenParagraph, InlineBoxGeometry, InlineBoxPlacement, LineBand, LineBands,
    LineGeometry, ParagraphContent, ParagraphShaper, ShapedParagraph, StyledRun, TextGeometry,
    TextMap,
};
use zgui_text_parley::{Controls, FontSystem, ShapedLayout, Shaper};
use zgui_text_style::{
    Direction, LengthPercent, OverflowWrap, ParagraphStyle, TextAlign, TextIndent, TextStyle,
    WordBreak, WrapMode,
};

/// Zeroes the counters and holds them for the duration of one test.
///
/// The counters are one global block, so the tests that read them take turns.
fn counting() -> MutexGuard<'static, ()> {
    let guard = counter::exclusive();
    counter::reset();
    guard
}

/// The shipped faces, loaded once for every case.
fn fonts() -> Arc<FontSystem> {
    static FONTS: OnceLock<Arc<FontSystem>> = OnceLock::new();
    FONTS.get_or_init(support::fonts).clone()
}

/// Words with ligatures, long words, a hyphen and Arabic, so that runs split, clusters join and
/// lines change direction.
const WORDS: &[&str] = &[
    "the",
    "fit",
    "office",
    "affluent",
    "flow",
    "a",
    "wraps",
    "supercalifragilistic",
    "x",
    "well-known",
    "1234",
    "ملف",
    "تقرير",
    "سنوي",
];

/// What goes between two words. The newline forces a break.
const GAPS: &[&str] = &[" ", " ", " ", "  ", "\u{a0}", "\n"];

/// One run's style, as drawn.
#[derive(Clone, Debug)]
struct RunSpec {
    size: f32,
    bold: bool,
    letter_spacing: f32,
    word_spacing: f32,
    wraps: bool,
    overflow_wrap: OverflowWrap,
    break_all: bool,
    brush: u32,
}

/// One atomic inline, as drawn.
#[derive(Clone, Debug)]
struct BoxSpec {
    /// Where in the text, as a fraction of its characters.
    at: f32,
    width: f32,
    height: f32,
    shift: f32,
}

/// One request of the sequence.
#[derive(Clone, Debug)]
struct Step {
    width: Option<f32>,
    probe: bool,
    banded: bool,
    /// A change of the first box's shift, which moves the box without moving a line.
    shift: f32,
    /// A new alignment from this step on.
    realign: Option<TextAlign>,
}

/// One whole case.
#[derive(Clone, Debug)]
struct Case {
    words: Vec<(usize, usize)>,
    cuts: Vec<f32>,
    runs: Vec<RunSpec>,
    boxes: Vec<BoxSpec>,
    align: TextAlign,
    rtl: bool,
    indent: (f32, f32, bool, bool),
    steps: Vec<Step>,
}

fn run_spec() -> impl Strategy<Value = RunSpec> {
    (
        prop::sample::select(vec![12.0f32, 13.5, 16.0, 21.0]),
        prop::bool::weighted(0.2),
        prop::sample::select(vec![0.0f32, 0.0, 0.5, -0.4]),
        prop::sample::select(vec![0.0f32, 0.0, 3.0]),
        prop::bool::weighted(0.9),
        prop::sample::select(vec![
            OverflowWrap::Normal,
            OverflowWrap::Normal,
            OverflowWrap::BreakWord,
            OverflowWrap::Anywhere,
        ]),
        prop::bool::weighted(0.1),
        0u32..3,
    )
        .prop_map(
            |(size, bold, letter_spacing, word_spacing, wraps, overflow_wrap, break_all, brush)| {
                RunSpec {
                    size,
                    bold,
                    letter_spacing,
                    word_spacing,
                    wraps,
                    overflow_wrap,
                    break_all,
                    brush,
                }
            },
        )
}

fn box_spec() -> impl Strategy<Value = BoxSpec> {
    (
        0.0f32..=1.0,
        prop::sample::select(vec![0.0f32, 0.0, 0.5, 7.25, 30.0]),
        prop::sample::select(vec![0.0f32, 12.0, 20.0]),
        prop::sample::select(vec![0.0f32, 3.0, -2.5]),
    )
        .prop_map(|(at, width, height, shift)| BoxSpec {
            at,
            width,
            height,
            shift,
        })
}

/// Widths that walk in small steps, as a drag does, with a jump now and then.
fn alignment() -> impl Strategy<Value = TextAlign> {
    prop::sample::select(vec![
        TextAlign::Start,
        TextAlign::Start,
        TextAlign::Left,
        TextAlign::End,
        TextAlign::Right,
        TextAlign::Center,
        TextAlign::Justify,
    ])
}

fn steps() -> impl Strategy<Value = Vec<Step>> {
    (
        20.0f32..420.0,
        prop::collection::vec(
            (
                prop::sample::select(vec![
                    0.0f32, 0.25, -0.25, 0.5, 1.0, 3.0, -3.0, 8.0, -8.0, 13.0,
                ]),
                prop::bool::weighted(0.05),
                prop::bool::weighted(0.25),
                prop::bool::weighted(0.08),
                prop::sample::select(vec![0.0f32, 0.0, 0.0, 4.0]),
                prop::bool::weighted(0.05),
                20.0f32..420.0,
                prop::option::weighted(0.1, alignment()),
            ),
            1..12,
        ),
    )
        .prop_map(|(start, deltas)| {
            let mut width = start;
            deltas
                .into_iter()
                .map(
                    |(delta, unbounded, probe, banded, shift, jump, to, realign)| {
                        width = if jump { to } else { (width + delta).max(1.0) };
                        Step {
                            width: (!unbounded).then_some(width),
                            probe,
                            banded,
                            shift,
                            realign,
                        }
                    },
                )
                .collect()
        })
}

fn case() -> impl Strategy<Value = Case> {
    (
        prop::collection::vec((0..WORDS.len(), 0..GAPS.len()), 1..40),
        prop::collection::vec(0.0f32..1.0, 0..4),
        prop::collection::vec(run_spec(), 1..5),
        prop::collection::vec(box_spec(), 0..4),
        alignment(),
        prop::bool::weighted(0.2),
        (
            prop::sample::select(vec![0.0f32, 0.0, 12.0, -6.0]),
            prop::sample::select(vec![0.0f32, 0.0, 0.1]),
            prop::bool::ANY,
            prop::bool::ANY,
        ),
        steps(),
    )
        .prop_map(
            |(words, cuts, runs, boxes, align, rtl, indent, steps)| Case {
                words,
                cuts,
                runs,
                boxes,
                align,
                rtl,
                indent,
                steps,
            },
        )
}

/// A case's content, owned.
struct Built {
    text: String,
    map: TextMap,
    runs: Vec<StyledRun>,
    /// The boxes, before any step changes a shift.
    boxes: Vec<InlineBoxGeometry>,
    paragraph: ParagraphStyle,
}

impl Built {
    fn new(case: &Case) -> Self {
        let mut text = String::new();
        for (index, &(word, gap)) in case.words.iter().enumerate() {
            if index > 0 {
                text.push_str(GAPS[gap]);
            }
            text.push_str(WORDS[word]);
        }
        let starts: Vec<usize> = text
            .char_indices()
            .map(|(at, _)| at)
            .chain([text.len()])
            .collect();
        let at = |fraction: f32| -> usize {
            let index = ((starts.len() - 1) as f32 * fraction).round() as usize;
            starts[index.min(starts.len() - 1)]
        };

        let mut cuts: Vec<usize> = case.cuts.iter().map(|&cut| at(cut)).collect();
        cuts.push(0);
        cuts.push(text.len());
        cuts.sort_unstable();
        cuts.dedup();
        let mut runs = Vec::new();
        for (index, range) in cuts.windows(2).enumerate() {
            let spec = &case.runs[index % case.runs.len()];
            runs.push(StyledRun {
                text: range[0]..range[1],
                style: Arc::new(TextStyle {
                    size: CssPx(spec.size),
                    weight: if spec.bold { 700.0 } else { 400.0 },
                    letter_spacing: LengthPercent::length(CssPx(spec.letter_spacing)),
                    word_spacing: LengthPercent::length(CssPx(spec.word_spacing)),
                    wrap_mode: if spec.wraps {
                        WrapMode::Wrap
                    } else {
                        WrapMode::NoWrap
                    },
                    overflow_wrap: spec.overflow_wrap,
                    word_break: if spec.break_all {
                        WordBreak::BreakAll
                    } else {
                        WordBreak::Normal
                    },
                    ..support::style()
                }),
                brush: PaintSlot(spec.brush),
            });
        }

        let mut boxes: Vec<InlineBoxGeometry> = case
            .boxes
            .iter()
            .map(|spec| InlineBoxGeometry {
                id: 0,
                offset: at(spec.at),
                width: CssPx(spec.width),
                height: CssPx(spec.height),
                ascent: CssPx(spec.height),
                shift: CssPx(spec.shift),
            })
            .collect();
        boxes.sort_by_key(|inline_box| inline_box.offset);
        for (index, inline_box) in boxes.iter_mut().enumerate() {
            inline_box.id = index as u64 + 1;
        }

        let mut map = TextMap::new();
        map.push(0..text.len(), 0, 0);
        let (length, percent, hanging, each_line) = case.indent;
        Self {
            text,
            map,
            runs,
            boxes,
            paragraph: ParagraphStyle {
                direction: if case.rtl {
                    Direction::RightToLeft
                } else {
                    Direction::LeftToRight
                },
                align: case.align,
                indent: TextIndent {
                    length: LengthPercent {
                        length: CssPx(length),
                        percent,
                    },
                    hanging,
                    each_line,
                },
                ..ParagraphStyle::initial()
            },
        }
    }

    /// The content with every box at `boxes`, under `paragraph`.
    fn content<'a>(
        &'a self,
        boxes: &'a [InlineBoxGeometry],
        paragraph: &'a ParagraphStyle,
    ) -> ParagraphContent<'a> {
        ParagraphContent {
            text: &self.text,
            map: &self.map,
            runs: &self.runs,
            boxes,
            paragraph,
            scale: 1.0,
        }
    }
}

/// The bands a banded step breaks into: a narrow first line beside a float, then the rest.
fn bands(width: f32) -> [LineBand; 2] {
    [
        LineBand {
            offset: CssPx(width * 0.3),
            max_advance: CssPx(width * 0.7),
        },
        LineBand::full(CssPx(width)),
    ]
}

/// Reads the line boxes and box placements out of the engine's laid-out form.
///
/// Written here against the engine's public reading interface, so it shares nothing with the
/// reading the shaper does and may keep.
fn read(engine: &ShapedLayout, has_boxes: bool) -> BrokenParagraph {
    let layout = &engine.layout;
    let prefix = engine.prefix;
    let mut lines = Vec::new();
    let mut boxes: SmallVec<[InlineBoxPlacement; 2]> = SmallVec::new();
    for (index, line) in layout.lines().enumerate() {
        let metrics = line.metrics();
        let ascent = metrics.ascent.round();
        let descent = metrics.descent.round();
        let above = ((metrics.line_height - (ascent + descent)) * 0.5).floor();
        let range = line.text_range();
        lines.push(LineGeometry {
            text: range.start.saturating_sub(prefix)..range.end.saturating_sub(prefix),
            top: CssPx(metrics.baseline - ascent - above),
            baseline: CssPx(metrics.baseline),
            height: CssPx(metrics.line_height),
            width: CssPx(metrics.advance),
            offset: CssPx(metrics.offset + metrics.inline_min_coord),
        });
        if has_boxes {
            for item in line.items() {
                if let parley::PositionedLayoutItem::InlineBox(placed) = item {
                    boxes.push(InlineBoxPlacement {
                        id: placed.id,
                        origin: Point::new(CssPx(placed.x), CssPx(placed.y)),
                        line: index,
                    });
                }
            }
        }
    }
    BrokenParagraph {
        geometry: Arc::new(TextGeometry {
            lines,
            size: Size::new(CssPx(layout.width()), CssPx(layout.height())),
            is_rtl: layout.is_rtl(),
        }),
        boxes,
    }
}

/// Every glyph and cluster the laid-out form gives, as text.
///
/// The debug form of a float prints the shortest text that reads back to the same bits, so equal
/// text means equal bits.
fn form(shaper: &Shaper, shaped: &ShapedParagraph<ShapedLayout>) -> Vec<String> {
    let mut out = Vec::new();
    for line in 0..=shaped.engine.layout.len() {
        let line = line as u16;
        shaper.visit_line(shaped, line, &mut |run| out.push(format!("{run:?}")));
        shaper.visit_clusters(shaped, line, &mut |run| out.push(format!("{run:?}")));
    }
    out
}

/// Bit-for-bit text of a broken result.
fn print(broken: &BrokenParagraph) -> String {
    format!("{broken:?}")
}

/// Breaks `case` at each of its widths on one shaped paragraph, and checks every answer.
fn check(case: &Case) -> Result<(), TestCaseError> {
    let built = Built::new(case);
    let fonts = fonts();
    let mut warm = Shaper::with_controls(fonts.clone(), Controls::Mark);
    let mut cold = Shaper::with_controls(fonts, Controls::Mark);
    let mut shaped = warm.shape(&built.content(&built.boxes, &built.paragraph));
    let has_boxes = !built.boxes.is_empty();
    let mut paragraph = built.paragraph;
    // Justifying moves cluster advances, and taking it back off later does not restore every bit.
    // So a paragraph once justified depends on the widths it was broken at before, and only its
    // own reading is a reference for it. No justified pass keeps lines.
    let mut history_free = true;

    for step in &case.steps {
        if let Some(align) = step.realign {
            paragraph.align = align;
        }
        history_free &= paragraph.align != TextAlign::Justify;
        let mut boxes = built.boxes.clone();
        if let Some(first) = boxes.first_mut() {
            first.shift = CssPx(first.shift.0 + step.shift);
        }
        let content = built.content(&boxes, &paragraph);
        let width = step.width.map(CssPx);
        let bands = step.width.map(bands);
        let mut request = BreakRequest::new(&content, width);
        if step.banded
            && let Some(bands) = &bands
        {
            request = request.banded(LineBands::new(bands));
        }
        if step.probe {
            request = request.probing();
        }

        let kept_before = counter::get(Counter::TextLinesKept);
        let answer = warm.break_lines(&mut shaped, &request);
        let kept = counter::get(Counter::TextLinesKept) > kept_before;
        let moved = shaped.broken_as() == Some(request.key());

        if moved {
            prop_assert_eq!(
                print(&answer),
                print(&read(&shaped.engine, has_boxes)),
                "the answer differs from a reading of the new break (kept: {})",
                kept
            );
        }
        if !history_free {
            continue;
        }
        // A fresh paragraph broken once at the same request is the reference.
        let mut fresh = cold.shape(&content);
        cold.break_lines(&mut fresh, &request);
        prop_assert_eq!(
            print(&answer),
            print(&read(&fresh.engine, has_boxes)),
            "the answer differs from a fresh break (kept: {})",
            kept
        );
        if moved {
            prop_assert_eq!(
                form(&warm, &shaped),
                form(&cold, &fresh),
                "the glyphs or clusters differ from a fresh break"
            );
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 384,
        ..ProptestConfig::default()
    })]

    /// Every answer of a breaking sequence equals a fresh break, bit for bit.
    #[test]
    fn a_rebreak_equals_a_fresh_break_bit_for_bit(case in case()) {
        let _guard = counting();
        check(&case)?;
    }
}

/// Narrow steps of a drag keep the lines, and a box that moves stops the keeping.
#[test]
fn a_small_width_step_keeps_the_lines_of_the_step_before() {
    let case = Case {
        words: (0..24).map(|index| (index % 7, 0)).collect(),
        cuts: vec![0.4],
        runs: vec![
            RunSpec {
                size: 16.0,
                bold: false,
                letter_spacing: 0.0,
                word_spacing: 0.0,
                wraps: true,
                overflow_wrap: OverflowWrap::Normal,
                break_all: false,
                brush: 0,
            },
            RunSpec {
                size: 16.0,
                bold: true,
                letter_spacing: 0.0,
                word_spacing: 0.0,
                wraps: true,
                overflow_wrap: OverflowWrap::Normal,
                break_all: false,
                brush: 1,
            },
        ],
        boxes: vec![BoxSpec {
            at: 0.5,
            width: 0.0,
            height: 0.0,
            shift: 0.0,
        }],
        align: TextAlign::Start,
        rtl: false,
        indent: (0.0, 0.0, false, false),
        steps: [300.0, 300.25, 300.5, 301.0]
            .into_iter()
            .map(|width| Step {
                width: Some(width),
                probe: false,
                banded: false,
                shift: 0.0,
                realign: None,
            })
            .chain([Step {
                width: Some(301.25),
                probe: false,
                banded: false,
                shift: 4.0,
                realign: None,
            }])
            .collect(),
    };
    {
        let _guard = counting();
        check(&case).expect("every answer equals a fresh break");
    }
    // The proptest above checks the answers; this checks that the keeping happens at all. The
    // first width costs a read, the three steps keep, and the moved box costs a read.
    let (drag, moved) = case.steps.split_at(4);
    assert_non_vacuous(
        Counter::TextLinesKept,
        Scenario::new("small width steps of a drag", || {
            let (rebroken, kept) = drive(&case, drag);
            assert_eq!((rebroken, kept), (4, 3));
        }),
        Scenario::new("a step that moves an inline box", || {
            let (rebroken, kept) = drive(&case, moved);
            assert_eq!((rebroken, kept), (1, 0));
        }),
    );
}

/// Shapes `case` and breaks it at `steps`. Returns how many breaks those steps took and kept.
fn drive(case: &Case, steps: &[Step]) -> (u64, u64) {
    let built = Built::new(case);
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let mut shaped = shaper.shape(&built.content(&built.boxes, &built.paragraph));
    let rebroken = counter::get(Counter::TextRebroken);
    let kept = counter::get(Counter::TextLinesKept);
    for step in steps {
        let mut boxes = built.boxes.clone();
        boxes[0].shift = CssPx(step.shift);
        let content = built.content(&boxes, &built.paragraph);
        shaper.break_lines(
            &mut shaped,
            &BreakRequest::new(&content, step.width.map(CssPx)),
        );
    }
    (
        counter::get(Counter::TextRebroken) - rebroken,
        counter::get(Counter::TextLinesKept) - kept,
    )
}
